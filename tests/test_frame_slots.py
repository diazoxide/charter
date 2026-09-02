"""Panels compose the renderers the status line already has.

Zones are not re-invented here: `statusline.py` already argues for identity in one place
and alerts in another, and a frame that split them differently would be a second layout to
keep in step with the first.

Width is measured, not theorised (the coordinator correction this task shipped under): a
panel process started as a tmux pane command inherits the *launching* shell's whole
environment, so `$COLUMNS` can describe a completely different rectangle than the pane
this process is actually drawing into. Measured against a real tmux 3.7c: a 22-column
pane, launched from a shell exporting `COLUMNS=200`, saw `COLUMNS='200'` in its own
environment. `Width` below pins that a panel lays out against its own tty instead.
"""

from __future__ import annotations

import contextlib
import os
import shutil
import sys
import unittest
from unittest import mock

from charter import config, instance, persona, statusline, todos, tui, workspace
from charter.frame import gather, layout, slots, state

from tests._isolation import PersonaIso
from tests._tmuxsocket import OPERATOR_SOCKET


def _row(name, *, branch="main", dirty=False, tracked_dirty=False, ahead=0, behind=0,
        ci=None, change=None, sigil="", current=False, repo=None,
        worktree_count=0) -> dict:
    """A `gather`-cache-shaped row — the exact fields `gather._entry` writes, built
    directly rather than through a real `git`/`gather.scan`: these tests pin `left`'s
    own COMPOSITION (what it does with a row already in the cache), which
    `tests/test_frame_gather.py` already covers the gather side of independently."""
    d = {"name": name, "branch": branch, "dirty": dirty, "tracked_dirty": tracked_dirty,
        "ahead": ahead, "behind": behind, "ci": ci, "change": change, "sigil": sigil,
        "current": current, "worktree_count": worktree_count}
    if repo is not None:
        d["repo"] = repo
    return d


def _plain_lines(out: str) -> list[str]:
    """*out* as the plain rows a terminal would show, split BEFORE the colour is stripped.

    The order is load-bearing and the obvious spelling is wrong: `tui.strip_ansi` runs
    `tui.sanitize` first, which turns a newline into a SPACE (it must — a wrapped line
    shears every column below it). So `strip_ansi(out).split("\\n")` returns ONE row
    however many the renderer emitted, and a test written that way cannot tell "the
    heading is the first line" from "the heading is somewhere in the pane" — which is
    exactly the difference every assertion about a multi-section sidebar is making.
    """
    return [tui.strip_ansi(ln) for ln in out.split("\n")]


def _seed(fid: str, **overrides) -> dict:
    data = {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
           "repos": [], "worktrees": []}
    data.update(overrides)
    gather.save(fid, data)
    return data


class Render(PersonaIso, unittest.TestCase):
    def test_top_names_the_workspace(self):
        out = slots.render("top", "f-1")
        self.assertTrue(out.strip())

    def test_bottom_renders(self):
        self.assertTrue(slots.render("bottom", "f-1").strip())

    def test_a_slot_never_exceeds_the_pane_width(self):
        """`tui.width` counts display cells, not characters — a wide glyph that fits by
        len() still wraps the pane and pushes the frame apart.

        **The pane is stated, not read off the machine** (#591). This used to bound the
        render by `tui.term_width(default=80)` while the render itself was laid out by
        `slots._width()` — and until #591 both of those read `$COLUMNS`, so the two sides
        of the assertion collapsed onto the same ambient value and the test could not fail
        whatever width it was given. That is #525's shape exactly. `_width` answers a
        constant now, so the bound has to come from somewhere that is not the shell
        either: the size is mocked, and 80 is what this pane IS.
        """
        with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size", return_value=os.terminal_size((80, 24))):
            for slot in ("top", "bottom"):
                for line in slots.render(slot, "f-1").splitlines():
                    with self.subTest(slot=slot):
                        self.assertLessEqual(tui.width(line), 80)

    def test_a_failing_renderer_yields_a_line_rather_than_an_exception(self):
        """A panel that raises leaves a hole in the frame; `statusline.render` makes the
        same promise for the same reason."""
        slots.SLOTS["boom"] = lambda fid: 1 / 0
        try:
            self.assertIn("charter", slots.render("boom", "f-1"))
        finally:
            del slots.SLOTS["boom"]

    def test_the_bottom_row_names_the_configured_hotkey_not_a_hardcoded_one(self):
        """`[frame] hotkey` is configurable and this row spelled `F2 palette` literally, so
        a plane on `hotkey = "F1"` had its own panel telling every operator the wrong
        key, on every repaint, forever.

        `F1` is chosen precisely because it is NOT the default: asserting against `F2`
        would pass against the hardcoded string this test exists to remove. The absence
        assertion is the one that fails on the mutation."""
        with mock.patch.dict(config.FRAME, {"hotkey": "F1"}):
            out = slots.render("bottom", "f-1")
        self.assertIn("F1 palette", out)
        self.assertNotIn("F2", out)

    def test_a_modifier_hotkey_reaches_the_panel_intact(self):
        """A second, differently-shaped value — `F1` alone could be satisfied by a
        one-character substitution. `M-m` shares no characters with `F2`."""
        with mock.patch.dict(config.FRAME, {"hotkey": "M-m"}):
            self.assertIn("M-m palette", slots.render("bottom", "f-1"))

    def test_a_frame_in_the_operators_own_tmux_advertises_no_hotkey(self):
        """Charter binds no key at all inside a tmux it did not start — a key table is
        server-wide in tmux with no per-window form, and taking one from every window
        the operator has open to reach a palette offering "Detach" (which their
        own prefix already does) is a worse trade than none. A panel still printing
        `F2 palette` there would be telling every operator about a key that does nothing,
        on every repaint, forever — the same defect
        `test_the_bottom_row_names_the_configured_hotkey_not_a_hardcoded_one` exists
        for, reached through the other server instead of the wrong config value."""
        state.record_server("f-in-tmux", OPERATOR_SOCKET)
        with mock.patch.dict(config.FRAME, {"hotkey": "F1"}):
            out = slots.render("bottom", "f-in-tmux")
        self.assertNotIn("palette", out)
        self.assertIn("todo", out, "the rest of the row is untouched")

    def test_a_frame_on_charters_own_server_still_advertises_it(self):
        """The other side of the same switch — a `_bottom` that simply stopped printing
        a hotkey would pass the test above on its own."""
        state.record_server("f-own", "charter")
        with mock.patch.dict(config.FRAME, {"hotkey": "F1"}):
            self.assertIn("F1 palette", slots.render("bottom", "f-own"))

    def test_an_unknown_slot_is_named_rather_than_drawn_blank(self):
        """`panel.run` (Task 7) refuses an unknown slot before ever spawning a pane for
        it — but `render` is the one place that can explain *why*, so it must not answer
        an unknown name with silence either."""
        out = slots.render("sideways", "f-1")
        self.assertIn("sideways", out)


class NoPanelDrawsItsOwnChrome(PersonaIso, unittest.TestCase):
    """tmux borders the pane; the panel fills it. #514's second candidate cause.

    A panel that drew its own box inside a pane tmux is already bordering gives a DOUBLE
    line by construction, and the two can never agree on colour — one comes from
    `commands_frame._CHROME`, the other from whatever the renderer chose. `statusline
    ._boxed` is the one thing in charter that draws such a box (`┌─┐│└┘` around its whole
    output), and inside a frame the status line it belongs to is suppressed outright
    (`statusline.a_frame_owns_this_surface`, ADR 0019) — so the frame is clean today, and
    this is what keeps it clean when a renderer next reaches for a helper.

    **Asked as "is this line ENCLOSED", never as "does it contain a box character".**
    Panels legitimately draw box-drawing glyphs: `statusline._TREE_MID`/`_TREE_END`/
    `_TREE_WT` are `├─ `, `└─ ` and `╰─ `, and the repo table is made of them. What
    `_boxed` does and a tree marker never does is put a glyph hard against BOTH edges of
    the same line. That is the property; the glyph is only a spelling.
    """

    #: `_boxed`'s own left and right edges — the top/bottom/rule rows (`┌ ┐`, `└ ┘`,
    #: `├ ┤`) and the body rows (`│ … │`) it wraps every line in.
    _LEFT = "┌│└├"
    _RIGHT = "┐│┘┤"

    def test_no_slot_returns_a_line_boxed_at_both_edges(self):
        for slot in slots.SLOTS:
            out = slots.render(slot, "f-1")
            for line in out.splitlines():
                bare = tui.strip_ansi(line).rstrip()
                if not bare:
                    continue
                with self.subTest(slot=slot, line=bare[:40]):
                    self.assertFalse(
                        bare[0] in self._LEFT and bare[-1] in self._RIGHT,
                        f"{slot} drew its own box inside a pane tmux already "
                        f"borders — a double rule, in two colours: {bare!r}")

    def test_the_check_would_catch_the_box_the_status_line_draws(self):
        """The control this file cannot do without: every assertion above is a NEGATIVE,
        and a negative passes just as well when the check is broken as when the code is
        right. `statusline._boxed` is the exact thing being excluded, so it is what the
        check is proved against — using the real function, not a hand-typed imitation of
        its output."""
        boxed = statusline._boxed("hello\nworld", 40).splitlines()
        self.assertTrue(boxed, "`_boxed` drew nothing to check against")
        for line in boxed:
            bare = tui.strip_ansi(line).rstrip()
            self.assertTrue(bare[0] in self._LEFT and bare[-1] in self._RIGHT,
                            f"the enclosure check does not recognise `_boxed`: {bare!r}")

    def test_a_tree_marker_is_not_mistaken_for_a_box(self):
        """The other control. `├─ ` opens a repo row and `╰─ ` opens a worktree's, so a
        check that merely looked for box characters would condemn the repo table — which
        is the panel's whole content."""
        for marker in (statusline._TREE_MID, statusline._TREE_END, statusline._TREE_WT):
            bare = f"{marker}charter".rstrip()
            self.assertFalse(bare[0] in self._LEFT and bare[-1] in self._RIGHT,
                             f"a tree row reads as a box: {bare!r}")


class Unimplemented(unittest.TestCase):
    """Which configured slots charter sizes but cannot draw — asked in one place because
    three callers need the same answer and must not drift: `cmd_launch` (to skip
    splitting a pane that would be permanently dead under `remain-on-exit on`),
    `frame_ready` (`--probe`) and `doctor.check_frame`."""

    def test_every_slot_charter_accepts_has_a_renderer(self):
        """The two registries have to agree: a slot `instance.FRAME_SLOTS` accepts and
        `slots.SLOTS` cannot draw is a pane charter splits and then leaves permanently
        dead. Asked of `FRAME_SLOTS` itself rather than a hand-written list, so retiring
        a slot from one registry and not the other (#488 retired `left` from both) is red
        rather than a frame with a hole in it."""
        self.assertEqual(slots.unimplemented(list(instance.FRAME_SLOTS)), [])

    def test_the_retired_sidebar_is_gone_from_the_registry_too(self):
        """#488. `left` is not a slot charter accepts any more, so it cannot reach here
        from config — but a preset or a hand-typed `charter panel left` still could, and
        this is the answer they get: named as unimplemented, which is what makes
        `_drawable_slots` skip it and `--probe`/`doctor` say so."""
        self.assertNotIn("left", slots.SLOTS)
        self.assertEqual(slots.unimplemented(["top", "left"]), ["left"])

    def test_an_all_implemented_configuration_names_nothing(self):
        self.assertEqual(slots.unimplemented(["top", "bottom"]), [])

    def test_the_answer_comes_from_the_registry_not_a_hardcoded_name(self):
        """Which slots have renderers TODAY is not the rule this function follows — the
        registry is. Proved from the other direction: temporarily REMOVE one from the
        registry and the answer must follow, exactly as it would the day a real slot's
        renderer regresses."""
        with mock.patch.dict(slots.SLOTS):
            del slots.SLOTS["right"]
            self.assertEqual(slots.unimplemented(["top", "right"]), ["right"])


class Width(unittest.TestCase):
    """Pins the coordinator correction directly against `_width`, independent of
    whatever `_top`/`_bottom` happen to render — so a future change to panel *content*
    can never accidentally paper over a regression in *how much room it thinks it has*.
    """

    def test_width_measures_the_panes_own_tty_ignoring_columns(self):
        with mock.patch.dict(os.environ, {"COLUMNS": "200"}), \
             mock.patch("os.get_terminal_size",
                         return_value=os.terminal_size((22, 5))):
            self.assertEqual(slots._width(), 22)

    def test_width_falls_back_to_a_constant_and_never_to_the_shells_columns(self):
        """#591. This used to assert the opposite — that `tui.term_width()` answers when
        there is no tty behind the fd — and calling that "the one case `$COLUMNS` is
        allowed to answer" was the defect written down as a property.

        There is no such case. `$COLUMNS` describes the LAUNCHING terminal (this module's
        own docstring measures a 22-column pane seeing `COLUMNS='200'`), so an
        unmeasurable pane laid out from it is laid out from a rectangle that is not this
        one — and `panel._hold`, the paint that runs when a panel has already failed, is
        the call site where that is worst. A pane nobody can measure gets
        :data:`slots._DEFAULT_COLS`, exactly as it has always got `_DEFAULT_ROWS` for its
        height.

        Both directions, because one alone is satisfiable by an accident: a `$COLUMNS`
        WIDER than the constant would wrap the paint out of its own pane, and a NARROWER
        one is the same wrong source pointing the other way (and would have been the
        arithmetic every renderer downstream then spends).
        """
        for columns in ("400", "55"):
            with self.subTest(columns=columns):
                with mock.patch.dict(os.environ, {"COLUMNS": columns}), \
                     mock.patch("os.get_terminal_size", side_effect=OSError("not a tty")):
                    self.assertEqual(slots._width(), slots._DEFAULT_COLS)

    def test_the_fallback_width_is_the_one_charter_already_falls_back_to(self):
        """The VALUE, which the two tests above cannot see: both assert against
        `_DEFAULT_COLS` itself, so setting it to 400 satisfies them and lays every
        unmeasurable pane out four hundred columns wide. Found by hand-mutating the
        constant — `tools/sweep.py` has no operator for a number (#569).

        Pinned as an agreement rather than as a literal, which is the same argument
        `_DEFAULT_ROWS`' own docstring makes about `panel._DEFAULT_ROWS`: "how big is a
        rectangle nobody can measure" must have ONE answer in charter, or the two copies
        are free to drift. `commands_frame._FALLBACK_SIZE` is the frame's own — the
        traditional default screen, 80x24 — and `tui.term_width`'s default is the same 80
        this used to reach through. Both dimensions, so a fix that lined one up and left
        the other is still red.
        """
        from charter import commands_frame
        self.assertEqual((slots._DEFAULT_COLS, slots._DEFAULT_ROWS),
                         commands_frame._FALLBACK_SIZE,
                         "a pane charter cannot measure is laid out at a different size "
                         "than the window charter cannot measure")

    def test_the_unmeasurable_pane_answers_the_same_for_both_dimensions(self):
        """The pair, said once. A pane with no tty behind it has neither a width nor a
        height to read, and before #591 those two facts came from different places —
        `_height` from a constant, `_width` from the environment — which is how one of
        them came to describe a different terminal than the other."""
        with mock.patch("os.get_terminal_size", side_effect=OSError("not a tty")):
            self.assertEqual((slots._width(), slots._height()),
                             (slots._DEFAULT_COLS, slots._DEFAULT_ROWS))


class RenderFollowsThePane(PersonaIso, unittest.TestCase):
    """The end-to-end version of `Width`, above: what a real panel process would see
    with a launching shell's `COLUMNS` still in its environment and its own pane far
    narrower. `PersonaIso` redirects `sys.stdout` to a `StringIO`, so `fileno()` is
    patched back to something callable rather than left to raise — the isolation harness
    would otherwise force every test onto the fallback branch and hide exactly the bug
    this class exists to catch.
    """

    def test_render_wraps_to_the_pane_not_to_the_wider_columns_value(self):
        with mock.patch.dict(os.environ, {"COLUMNS": "200"}), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                         return_value=os.terminal_size((10, 3))):
            for slot in ("top", "bottom"):
                out = slots.render(slot, "f-1")
                with self.subTest(slot=slot):
                    for line in out.splitlines():
                        self.assertLessEqual(tui.width(line), 10)


class WideGlyphs(PersonaIso, unittest.TestCase):
    """`tui.width` counts display cells, not `len()` — every other test in this file
    uses pure-ASCII content, where the two coincide, so none of them would notice a
    future edit that swapped `tui.truncate` for character slicing (`x[:w]`). Caught in
    review by a hand-built probe: a workspace name of CJK glyphs measuring 57 display
    cells rendered untouched into a 30-cell pane. This pins the same shape with an
    assertion, not a probe: 30 CJK characters are 60 display cells (two each) but only
    30 *characters* — half the false margin `len()`-based slicing would report as safe
    against a pane this narrow.
    """

    def test_a_cjk_workspace_name_still_fits_a_narrow_pane(self):
        cjk = "測" * 30  # 30 characters, 60 display cells
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": cjk}), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                         return_value=os.terminal_size((20, 3))):
            line = slots.render("top", "f-1")
        self.assertLessEqual(tui.width(line), 20)


class TopRenderer(PersonaIso, unittest.TestCase):
    """Task 4 (#385) investigated `top`'s "context/session when available" bullet —
    `statusline._session_strip`/`_context_gauge` — and found neither can produce
    anything from a panel process: both are gated at every branch on Claude Code's
    per-turn stdin payload, which only `statusline.main` ever receives, never a
    long-lived tmux pane command. See `slots._top`'s own docstring for the full
    argument. These tests pin that finding as a regression rather than a one-time
    observation: a future edit that wires either call back into `_top` "because it
    compiles" must turn one of these red, since the call would return `[]` forever and
    add nothing but a docstring gone stale.

    **#413 gave `top` a gauge anyway, and none of that changed.** What changed is that a
    panel now has a FILE to read (`statusline.recorded_context_gauge`), so the gauge is
    composed from the recorded history rather than from a payload nobody hands it. The two
    "never called" tests below are exactly as load-bearing as before: reaching for the
    payload-gated helpers would still return `[]` forever, and now it would do it beside a
    working gauge, which is harder to notice rather than easier."""

    def test_the_context_gauge_is_never_called_by_top(self):
        with mock.patch("charter.statusline._context_gauge") as gauge:
            slots.render("top", "f-1")
        gauge.assert_not_called()

    def test_the_session_strip_is_never_called_by_top(self):
        with mock.patch("charter.statusline._session_strip") as strip:
            slots.render("top", "f-1")
        strip.assert_not_called()

    def test_context_gauge_confirmed_empty_with_no_payload_the_premise_top_relies_on(self):
        """The load-bearing fact behind not reaching for the payload-gated helper: with no
        live per-turn payload — exactly what a panel process always has — `_context_gauge`
        produces nothing, not a misleading zero. `tests/test_statusline_gauge.py` already
        pins this generically (`test_silent_before_first_api_call`); repeated here because
        it is the premise `_top`'s design decision rests on."""
        self.assertEqual(statusline._context_gauge(None), [])


class EveryPanelDrawsTheFramesOwnWorkspace(PersonaIso, unittest.TestCase):
    """#512 — the reported bug, at the layer that showed it.

    An operator opened a session and the frame's repo table was empty; the repos appeared
    "after I resized the window". The resize was a coincidence (it happened alongside a
    tool call, and a tool call's `posttooluse` hook refreshes the gather cache from inside
    the HARNESS). What was actually wrong is that **a panel resolved a different workspace
    than the launcher did**: measured on the reporting plane, three per-terminal pointers
    naming `harness-wrapper`, one naming `user-reporting`, and a `default` workspace with
    no clones in it at all. `workspace.resolve` from inside a panel pane cannot reach any
    of the rungs that decided it (`state.record_workspace`'s docstring walks all six), so
    every panel drew `default` — its empty repo list, its todo count, its alerts — into a
    frame the launcher had built for another workspace entirely.

    The fixture below is that situation exactly: the panel's OWN `workspace.resolve()`
    answers `default`, and the frame was launched for `elsewhere`. Nothing here mocks a
    renderer or a helper — `workspace.resolve` is left completely alone and answers
    honestly for this process, which is the whole point: the defect was never that
    `resolve` was wrong, it was that a panel asked it at all.
    """

    OTHER = "elsewhere"

    def _render(self, slot, fid="f-1", *, cols=200, rows=24) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return tui.strip_ansi(slots.render(slot, fid))

    def setUp(self) -> None:
        super().setUp()
        # A panel process standing at the plane root, with no pin and no pointer it can
        # read — `workspace.resolve()` genuinely answers the built-in default here, which
        # is what a real panel gets.
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))
        self.assertEqual(workspace.resolve(), config.DEFAULT_WORKSPACE,
                         "the fixture no longer reproduces a panel's own answer")

    def test_top_names_the_workspace_the_frame_was_launched_for(self):
        state.record_workspace("f-1", self.OTHER)
        out = self._render("top")
        self.assertIn(self.OTHER, out)
        self.assertNotIn(config.DEFAULT_WORKSPACE, out,
                         "the header named the panel's own guess, not the frame's")

    def test_the_attention_row_counts_the_frames_workspaces_todos(self):
        """The same defect one field over, and the reason `_bottom` goes through
        `_frame_workspace` too rather than only the table: a row reading `0 todos` beside
        a table of another workspace's repos is two answers to one question."""
        todos.add(self.OTHER, "something the frame's workspace is carrying")
        state.record_workspace("f-1", self.OTHER)
        _seed("f-1")
        self.assertIn("1 todo", self._render("bottom"))

    def test_without_the_record_it_falls_back_to_resolving_locally(self):
        """The migration case, and the failed-write case. `None` from
        `state.frame_workspace` means "do not take it from here" — the fallback is exactly
        what every panel did before #512, so this is never worse than what it replaces."""
        self.assertIsNone(state.frame_workspace("f-1"))
        self.assertIn(config.DEFAULT_WORKSPACE, self._render("top"))

    def test_a_workspace_chosen_inside_the_frame_no_longer_moves_the_panels(self):
        """**This case was the inverse until #791, and `docs/frame.md` is corrected with
        it.** The promise it used to assert was: "`charter workspace use <name>` typed at
        the agent moves the panels too — the pointer is written under the frame's id and
        the panels read it back under the same one."

        What that made the pointer was a rung of `state.own_workspace`, which since #733
        is also what decides MEMBERSHIP — so the command re-homed the chat. Measured on
        three chat directories: `charter workspace use gamma` typed inside `alpha.1` left
        `alpha.2` unable to see it (`chats.others` empty, `frame-chat alpha.1` answering
        `no chat 'alpha.1' here`) and put `gamma`'s chats on `alpha.1`'s bar, where
        `cmd_chat` refuses every one of them. That is #733 and #788 verbatim, and §4j
        forbids it: `{workspace}-{hash}` is identity, not a property.

        So the panels draw what the LAUNCH recorded, and only a launch can move it. Same
        writer as before — the real `workspace.set_active` with the frame's id in the
        environment, because that is exactly what `charter workspace use` does — and the
        assertion is the one that is now true.
        """
        state.record_workspace("f-1", self.OTHER)
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}):
            self.assertNotEqual(workspace.set_active("chosen-later"), "locked")
        out = self._render("top")
        self.assertIn(self.OTHER, out)
        self.assertNotIn("chosen-later", out,
                         "a pointer typed inside the frame moved the chat's own workspace")

    def test_the_command_still_moves_what_the_frames_own_shell_acts_on(self):
        """**The other half, and without it the case above would be satisfied by deleting
        the write.** `charter workspace use` answers "which workspace is this session
        working in", and that is a live question inside a frame: it is what every
        `charter clone`, `charter repos` and `charter ws current` in the agent's own shell
        then acts on.

        Refusing the command inside a frame was the alternative and it is rejected on
        evidence: the only test for "inside a frame" is `session.current()`, and every
        agent spawned from a frame inherits `$CHARTER_SESSION_ID` — so the refusal would
        fire on agents doing ordinary CLI work in isolated worktrees. The pointer is
        written, the panels ignore it, and those are two different questions rather than
        one broken answer."""
        state.record_workspace("f-1", self.OTHER)
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}):
            workspace.set_active("chosen-later")
            self.assertEqual(workspace.resolve(cwd=config.ROOT), "chosen-later")
        self.assertEqual(workspace.for_session("f-1"), "chosen-later")

    def test_the_frames_repo_table_does_not_follow_that_choice_either(self):
        """The same rung, on the surface #512 is actually about — a header that moved and
        a table that did not would be the disagreement this class exists to prevent, and
        that is as true of the direction #791 turned it as of the old one."""
        clone = config.WORKSPACES_DIR / "chosen-later" / "arepo"
        (clone / ".git").mkdir(parents=True)
        state.record_workspace("f-1", self.OTHER)
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}):
            workspace.set_active("chosen-later")
        self.assertEqual(slots.repos_rows_wanted("f-1", pane_cols=200), 0)

    def test_the_pane_is_sized_from_the_frames_workspace_too(self):
        """`gather.row_count`'s no-cache path is the third surface that used to resolve
        for itself, and its caller makes it matter: `cmd_resize` runs as a tmux
        `run-shell` child on every step of a terminal drag, with the SERVER's environment
        and neither the operator's cwd nor their pane id. A pane sized from one workspace
        while its panel draws another is #512 wearing a height instead of a row.

        Two real workspaces on disk, one with a clone in it and one without, so the count
        can only come from the right listing — and no cache, which is the state
        `cmd_launch` guarantees by calling `gather.discard`."""
        clone = config.WORKSPACES_DIR / self.OTHER / "arepo"
        (clone / ".git").mkdir(parents=True)
        self.assertEqual(slots.repos_rows_wanted("f-1", pane_cols=200), 0,
                         "the fixture already counted a repo before the record existed")
        state.record_workspace("f-1", self.OTHER)
        self.assertEqual(slots.repos_rows_wanted("f-1", pane_cols=200), 1 + 1)

    def test_a_corrupt_record_falls_back_rather_than_drawing_it(self):
        """`frame_workspace` name-checks on read, so a `workspaces/` escape never reaches
        `workspace_dir()`'s join or the operator's screen — and the panel degrades to its
        own answer rather than to nothing at all."""
        d = state.frame_dir("f-1", create=True)
        (d / "workspace").write_text("../../escaped\n")
        out = self._render("top")
        self.assertNotIn("escaped", out)
        self.assertIn(config.DEFAULT_WORKSPACE, out)


class TheChipAndItsStarNameTheSameWorkspace(PersonaIso, unittest.TestCase):
    """`top`'s `⬢ <name>*` is one claim, not two, and the `*` is the half that says who
    made it: "`$CHARTER_WORKSPACE` chose this".

    The two halves used to be answered from different rungs — the name from
    `state.workspace_for`, the star from `workspace.source()`, which only knows whether
    the variable is set at all. A frame launched under the pin that then had `charter
    workspace use other` typed at it drew `⬢ other*`: a name the environment did not
    name, wearing the marker that says it did, while every command in that session went
    on acting in the pinned workspace. Self-contradictory on its own line, and the
    contradiction is the tell — an operator reading it has no way to know which half to
    believe.

    This is the case charter's own documentation steers people into: `hooks`' nudge tells
    an operator to "re-launch with `CHARTER_WORKSPACE=<name>` set" to aim a parallel or
    unattended agent, and `commands_workspace` warns that `ws use` will not stick while
    it is. The frame has to survive somebody doing both.
    """

    def _top(self, fid="f-1", *, cols=200, rows=24) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return tui.strip_ansi(slots.render("top", fid))

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))

    def test_the_pinned_name_is_the_one_drawn_and_it_keeps_the_star(self):
        """Through the real `workspace.set_active` with the frame's id in the
        environment, because that is what `charter workspace use` does — a hand-written
        pointer file would prove the reader reads a file, not that a real in-frame command
        can no longer move the header off the pin.

        **The launch record names a THIRD workspace, and since #791 it has to.** The
        fixture used to record the pin's own name and let the pointer be the only
        disagreeing rung; with the pointer no longer a rung, `state.workspace_for` would
        have answered `zeta` from the record whether the pin was read or not, and this case
        would have gone on passing while measuring neither the pin nor the star."""
        os.environ["CHARTER_WORKSPACE"] = "zeta"
        state.record_workspace("f-1", "recorded-at-launch")
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}):
            workspace.set_active("other", force=True)
        self.assertEqual(workspace.for_session("f-1"), "other",
                         "the fixture never wrote the pointer this test is about")
        out = self._top()
        self.assertIn("⬢ zeta*", out)
        self.assertNotIn("other", out,
                         "the header named a workspace no command in the session acts on")
        self.assertNotIn("recorded-at-launch", out,
                         "the header named the launch record over the pin above it")
        self.assertEqual(workspace.resolve(), "zeta",
                         "the header and the session's own commands disagree")

    def test_a_name_the_pin_did_not_choose_never_wears_the_star(self):
        """The star's meaning, checked from the other side: a pin charter refuses to draw
        (`workspace_dir()` would join it) leaves the panel on a name the variable did not
        name, and the marker has to come off with it. A star sourced from "is the variable
        set" would still be there."""
        os.environ["CHARTER_WORKSPACE"] = "../../escaped"
        state.record_workspace("f-1", "recorded-at-launch")
        out = self._top()
        self.assertIn("⬢ recorded-at-launch", out)
        self.assertNotIn("escaped", out)
        self.assertNotIn("*", out, "the star claimed the environment chose this name")

    def test_no_pin_means_no_star(self):
        """The ordinary frame, so neither test above can pass by the star never being
        drawn at all."""
        state.record_workspace("f-1", "recorded-at-launch")
        out = self._top()
        self.assertIn("⬢ recorded-at-launch", out)
        self.assertNotIn("*", out)


class TopDrawsTheRecordedGauge(PersonaIso, unittest.TestCase):
    """#413: `ctx NN%` / `cache NN%` on `top`, out of the history the suppressed status
    line records — the one capability a framed Claude Code session lost to #386 and the
    thing 0.52.0's own news entry named as "genuinely lost".

    Every test renders through the real `slots.render("top", …)`, because what #413
    promises is that a PANEL shows this. And every "does not know" case is asserted to
    draw NOTHING rather than a zero: that rule (`_top`'s own docstring, and the task
    brief's) is what the whole design is built around, so it is pinned case by case
    rather than once.
    """

    def setUp(self):
        super().setUp()
        self.fid = "gauge-frame"
        self.sid = "cc-session-1"

    def _usage(self, *rows: str) -> None:
        f = statusline._usage_file(self.sid)
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text("\n".join(rows) + "\n")

    def _render(self) -> str:
        with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((200, 3))):
            return tui.strip_ansi(slots.render("top", self.fid))

    def test_a_recorded_session_puts_the_gauge_on_the_row(self):
        state.record_harness_session(self.fid, self.sid)
        self._usage("900,100,90,42")
        out = self._render()
        self.assertIn("ctx 42%", out)
        self.assertIn("cache 90%", out)

    def test_the_workspace_and_the_persona_are_not_pushed_off_by_it(self):
        """The gauge JOINS `top`'s identity row; it does not replace it. `top` answers
        "where am I and who am I being" first, and a session number that evicted either
        would leave the row not earning its line."""
        state.record_harness_session(self.fid, self.sid)
        self._usage("900,100,90,42")
        self.assertIn("⬢", self._render())

    def test_a_frame_with_no_recorded_session_draws_no_gauge(self):
        """Every frame whose harness is not Claude Code, and every frame launched by a
        charter that predates the mapping. Nothing is handed a usage payload there, so
        there is nothing to draw — and `ctx 0%` would be a claim about a session charter
        has never seen a single number from."""
        self._usage("900,100,90,42")
        out = self._render()
        self.assertNotIn("ctx", out)
        self.assertNotIn("cache", out)

    def test_a_recorded_session_with_no_turns_yet_draws_no_gauge(self):
        """Early in a session, and right after `/compact`: the mapping is written on the
        first render, the numbers only once the API has answered."""
        state.record_harness_session(self.fid, self.sid)
        out = self._render()
        self.assertNotIn("ctx", out)
        self.assertNotIn("cache", out)

    def test_a_history_written_before_the_ctx_field_existed_draws_no_ctx(self):
        """An upgrade mid-session is the ordinary case: three-field rows are what every
        charter before #413 wrote. The cache half is still fully derivable from them and
        is drawn; the context percentage was never recorded and is not guessed at."""
        state.record_harness_session(self.fid, self.sid)
        self._usage("900,100,90")
        out = self._render()
        self.assertNotIn("ctx", out)
        self.assertIn("cache 90%", out)

    def test_a_turn_recorded_without_a_percentage_falls_back_to_the_last_one_seen(self):
        """A turn early in a session carries usage but no percentage, so its ctx field is
        empty. Blanking a gauge that was correct one turn ago is worse than showing the
        most recent figure charter actually saw — and the ring buffer is only
        `_TREND_KEEP` turns deep, so it cannot drift far."""
        state.record_harness_session(self.fid, self.sid)
        self._usage("800,200,80,37", "900,100,90,")
        self.assertIn("ctx 37%", self._render())

    def test_the_gauge_survives_a_terse_density(self):
        """`terse` buys back columns by dropping the charter version — a standing fact
        about the install. The gauge is the opposite: the one field on this row that is
        different every turn."""
        state.record_harness_session(self.fid, self.sid)
        self._usage("900,100,90,42")
        state.record_density(self.fid, "minimal")
        self.assertIn("ctx 42%", self._render())

    def test_it_reads_the_session_the_frame_recorded_and_not_some_other(self):
        """The mapping is the whole mechanism, so this pins that it is actually followed
        rather than the panel finding a usage file some other way: two sessions with
        different numbers on disk, and the row must show the one this frame is mapped
        to."""
        other = statusline._usage_file("cc-session-2")
        other.parent.mkdir(parents=True, exist_ok=True)
        other.write_text("100,900,10,99\n")
        state.record_harness_session(self.fid, self.sid)
        self._usage("900,100,90,42")
        out = self._render()
        self.assertIn("ctx 42%", out)
        self.assertNotIn("99%", out)

    def test_a_new_frame_claiming_a_recycled_pid_does_not_inherit_the_gauge(self):
        """#383's recycled-pid adoption, applied to the sharpest of the three files it
        clears. A frame id is `<workspace>-<launcher pid>`, and a launcher landing on a
        pid an earlier launcher used adopts that frame's whole directory — so without
        `clear_shape` taking `session` with it, a brand-new session's `top` row would
        draw the PREVIOUS session's `ctx 42%` as its own, confidently, and forever: the
        session that would correct it is over."""
        state.record_harness_session(self.fid, self.sid)
        self._usage("900,100,90,42")
        state.clear_shape(self.fid)
        self.assertNotIn("ctx", self._render())
        self.assertEqual(statusline._context_gauge({}), [])


class BottomRenderer(PersonaIso, unittest.TestCase):
    """`bottom` composes a fourth field now — `statusline._session_news` — alongside
    the todo count, the top alert, and the hotkey hint already there. See
    `slots._bottom`'s own docstring for the priority-drop design and why a single
    trailing `tui.truncate` over the whole joined line is not enough by itself."""

    def test_session_news_appears_alongside_the_alerts(self):
        with mock.patch("charter.statusline._session_news", return_value=["⛊ 1 denied"]), \
             mock.patch("charter.statusline._alerts", return_value=["⚠ something"]):
            out = tui.strip_ansi(slots.render("bottom", "f-1"))
        self.assertIn("⛊ 1 denied", out)
        self.assertIn("⚠ something", out)

    def test_session_news_uses_this_panels_own_session_id(self):
        """A panel has no per-turn payload to hand `_session_news` a session id — the
        identical point `_top`'s docstring makes about the gauge. `session.current()`
        supplies it, the same fallback `_right` already trusts for `_persona_chips`.
        Pinned with a sentinel id nothing in `_bottom` could have produced on its own,
        and requiring it actually reach the call.

        `inflight=False` travels with it since #387: the panel draws the in-flight
        tracker itself, as a spinner that moves, and asking `_session_news` for its own
        `⚡ N` too would print one fact twice on one row. Asserted here rather than in a
        test of its own because it is part of THIS call, and a change that dropped it
        would still pass a looser `assert_called_once()`."""
        with mock.patch("charter.session.current", return_value="SID-SENTINEL-0xF00D"), \
             mock.patch("charter.statusline._session_news", return_value=[]) as news:
            slots.render("bottom", "f-1")
        news.assert_called_once_with("SID-SENTINEL-0xF00D", inflight=False)

    def test_never_exceeds_the_pane_width(self):
        with mock.patch("charter.statusline._session_news",
                        return_value=["⛊ 1 denied", "⚡ 2"]), \
             mock.patch("charter.statusline._alerts",
                        return_value=["⚠ reinit: charter ws reinit needed right now"]), \
             mock.patch("charter.statusline._todo_count", return_value=7), \
             mock.patch("os.get_terminal_size", return_value=os.terminal_size((22, 3))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("bottom", "f-1")
        for line in out.splitlines():
            self.assertLessEqual(tui.width(line), 22)

    def test_a_starved_pane_drops_the_hotkey_and_todo_before_the_alert_or_news(self):
        """Sweeps a few widths against the same four fields, matching
        `LeftRenderer.test_a_starved_pane_drops_lowest_priority_fields_first`'s style:
        the alert survives everywhere it is asked to, and the hotkey — the one thing
        always rediscoverable another way — is first to give up its columns."""
        with mock.patch("charter.statusline._alerts", return_value=["AAAAA"]), \
             mock.patch("charter.statusline._session_news", return_value=["NNNNN"]), \
             mock.patch("charter.statusline._todo_count", return_value=3), \
             mock.patch.dict(config.FRAME, {"hotkey": "F2"}):
            for w, want_alert, want_news, want_todo, want_hotkey in (
                (80, True, True, True, True),
                (10, True, False, False, False),
                (20, True, True, False, False),
            ):
                with mock.patch("os.get_terminal_size",
                                return_value=os.terminal_size((w, 3))), \
                     mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
                    out = tui.strip_ansi(slots.render("bottom", "f-1"))
                with self.subTest(width=w):
                    self.assertLessEqual(tui.width(out), w)
                    self.assertEqual("AAAAA" in out, want_alert)
                    self.assertEqual("NNNNN" in out, want_news)
                    self.assertEqual("3 todos" in out, want_todo)
                    self.assertEqual("F2 palette" in out, want_hotkey)

    def test_a_failing_session_news_call_yields_a_line_rather_than_an_exception(self):
        with mock.patch("charter.statusline._session_news",
                        side_effect=RuntimeError("boom")):
            self.assertIn("charter", slots.render("bottom", "f-1"))

    def test_empty_fields_leave_no_stray_separator(self):
        """No alerts, no session news — `_fit_fields` must SKIP an empty field rather
        than keep it and let ` · `.join emit a blank slot between separators
        (`"5 todos ·  · F2 palette"`, say). Asserts the exact string rather than just
        `in`/`not in`, so a stray separator cannot hide inside a substring match."""
        with mock.patch("charter.statusline._alerts", return_value=[]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=5), \
             mock.patch.dict(config.FRAME, {"hotkey": "F2"}):
            out = tui.strip_ansi(slots.render("bottom", "f-1"))
        self.assertEqual(out, "5 todos · F2 palette")

    def test_a_switch_outcome_is_drawn_on_this_row(self):
        """#729. The outcome of an F2 choice was a `display-message`, which suspends the
        client's pane redraw for its whole duration — measured at 4.03s of frozen screen
        on tmux 3.7c and 3.99s at the 3.2 floor, spent hiding the repaint it announced.
        It is a field of this row instead."""
        state.say("f-1", "charter: workspace \u2192 gamma")
        self.assertIn("charter: workspace \u2192 gamma",
                      tui.strip_ansi(slots.render("bottom", "f-1")))

    def test_an_expired_outcome_leaves_the_row_as_it_was(self):
        """The dwell is the whole reason this may sit at the top of the priority order:
        it gives the row back. Without an expiry it would sit on top of an `_alerts()`
        entry — an actionable problem carrying its own fix — for the rest of the frame's
        life, which is #727's defect wearing #729's clothes."""
        with mock.patch("charter.statusline._alerts", return_value=[]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=5), \
             mock.patch.dict(config.FRAME, {"hotkey": "F2"}):
            state.say("f-1", "charter: long gone", seconds=-1)
            out = tui.strip_ansi(slots.render("bottom", "f-1"))
        self.assertEqual(out, "5 todos \u00b7 F2 palette")

    def test_the_outcome_outranks_every_other_field_on_a_starved_pane(self):
        """It is the direct answer to the last thing the operator DID, and it is the only
        field here that is about an instant rather than a state — a row too narrow to say
        both should say the one that will not be true in a moment. `terse` asks the same
        question through `limit=1` and gets the same answer."""
        state.say("f-1", "charter: NOTICED")
        with mock.patch("charter.statusline._alerts", return_value=["AAAAA"]), \
             mock.patch("charter.statusline._session_news", return_value=["NNNNN"]), \
             mock.patch("charter.statusline._todo_count", return_value=3), \
             mock.patch("os.get_terminal_size", return_value=os.terminal_size((20, 3))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = tui.strip_ansi(slots.render("bottom", "f-1"))
        self.assertIn("NOTICED", out, "the outcome lost its columns to a lower field")
        self.assertNotIn("AAAAA", out)

    def test_a_frame_with_nothing_to_say_draws_the_row_it_always_drew(self):
        """An empty field is dropped whole, so the notice must cost nothing at all on the
        overwhelming majority of repaints, when no switch has just happened."""
        with mock.patch("charter.statusline._alerts", return_value=[]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=5), \
             mock.patch.dict(config.FRAME, {"hotkey": "F2"}):
            out = tui.strip_ansi(slots.render("bottom", "f-1"))
        self.assertEqual(out, "5 todos \u00b7 F2 palette")

    def test_the_outcome_belongs_to_ITS_frame_and_no_other(self):
        """The half `display-message` could not do at all. `-t <pane>` picks the FORMAT
        target, not the client: measured on tmux 3.7c and 3.2 alike, a message aimed at a
        pane of session `sa` was drawn on the terminal attached to `sb`. A row reads its
        own frame's state, so a second frame cannot see the first one's outcome."""
        state.say("f-1", "charter: FOR-F1-ONLY")
        self.assertNotIn("FOR-F1-ONLY", tui.strip_ansi(slots.render("bottom", "f-2")))

    def test_the_todo_count_still_shows_at_zero_unlike_the_new_news_field(self):
        """`todo` predates Task 4 and keeps its own, different presence rule: `_bottom`
        showed `0 todo` even at zero before this task touched the function, unlike
        `_session_news`'s "silent unless something happened" discipline — Task 4 must
        not quietly fold the two together. Caught for real during this task: an earlier
        draft gated `todo_text` on `if todos`, which passed every test in this file (all
        of them mock a non-zero todo count) and only broke three tests in OTHER files
        (`test_frame_panel.py`, `test_frame_tmux_integration.py`) that render `bottom`
        against a real, empty environment where the todo count is genuinely 0."""
        with mock.patch("charter.statusline._todo_count", return_value=0):
            out = tui.strip_ansi(slots.render("bottom", "f-1"))
        self.assertIn("0 todo", out)

    def test_a_failing_session_current_call_yields_a_line_rather_than_an_exception(self):
        with mock.patch("charter.session.current", side_effect=RuntimeError("boom")):
            self.assertIn("charter", slots.render("bottom", "f-1"))

    def test_never_exceeds_the_pane_width_even_narrower_than_a_single_field(self):
        """`_fit_fields` always force-keeps the first non-empty field so the row is
        never blank on its own account (mirrors `_row`'s `max(1, ...)` branch floor) —
        which means the trailing `tui.truncate` at the call site, not the budgeting
        loop, is what actually protects a pane narrower than that one field. Mirrors
        `LeftRenderer.test_never_exceeds_the_pane_width_even_when_narrower_than_the_markers`."""
        with mock.patch("charter.statusline._alerts",
                        return_value=["a very long alert that will not fit at all"]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=0), \
             mock.patch("os.get_terminal_size", return_value=os.terminal_size((3, 3))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("bottom", "f-1")
        self.assertLessEqual(tui.width(out), 3)


class MinimalStillSaysHowToDriveTheFrame(PersonaIso, unittest.TestCase):
    """#743. `density = minimal` is the arrangement with no repo table, no sidebar and no
    charter version — two one-row strips, with the repo table, the todo list, the
    workspace switch, the persona switch and the way back to `full` ALL behind `F2`. It
    was also the one arrangement that stopped saying `F2` anywhere on screen.

    The cause is that the hint was ranked against the other fields, and `terse` keeps one.
    The hint is a different kind of thing from the other three: an alert, a spinner and a
    todo count are news about the plane, and the hint is the one piece of chrome that says
    how to drive the frame. Ranking it against news is what dropped it from the only frame
    that needed it.
    """

    def _row(self, fid: str) -> str:
        with mock.patch("charter.statusline._alerts", return_value=[]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=7), \
             mock.patch.dict(config.FRAME, {"hotkey": "F2"}):
            return tui.strip_ansi(slots.render("bottom", fid))

    def test_a_minimal_frame_still_advertises_the_palette(self):
        """The report, exactly: `7 todos · F2 palette` before, `7 todos` after."""
        state.record_density("f-min", "minimal")
        self.assertEqual(slots.verbosity("f-min"), "terse")
        self.assertEqual(self._row("f-min"), "7 todos · F2 palette")

    def test_the_density_still_trims_everything_that_is_news(self):
        """The exemption is one field, not the end of the trim. With something running and
        something wrong, a terse row still says exactly one of them — the highest-priority
        — and still says how to open the palette."""
        state.record_density("f-min2", "minimal")
        with mock.patch("charter.statusline._alerts", return_value=["⚠ reinit needed"]), \
             mock.patch("charter.statusline._session_news", return_value=["⛊ 1 denied"]), \
             mock.patch("charter.statusline._todo_count", return_value=7), \
             mock.patch.dict(config.FRAME, {"hotkey": "F2"}):
            out = tui.strip_ansi(slots.render("bottom", "f-min2"))
        self.assertEqual(out, "⚠ reinit needed · F2 palette")

    def test_a_notice_takes_the_one_news_slot_and_the_hint_still_shows(self):
        """The composition with #763, which landed on this same call while this was open.

        A notice is the outcome of the last thing the operator CHOSE, and it is now the
        top-priority field — so at `minimal` it is the one piece of news that survives,
        outranking even an alert for the few seconds it dwells. The hint is not news and
        does not compete with it: both are on the row. Asserted because two changes met at
        one line, and "each is right alone" is not the same claim as "the pair is right".
        """
        state.record_density("f-min-n", "minimal")
        state.say("f-min-n", "charter: workspace → gamma")
        with mock.patch("charter.statusline._alerts", return_value=["⚠ reinit needed"]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=7), \
             mock.patch.dict(config.FRAME, {"hotkey": "F2"}):
            out = tui.strip_ansi(slots.render("bottom", "f-min-n"))
        self.assertEqual(out, "charter: workspace → gamma · F2 palette")

    def test_the_hint_follows_the_configured_key(self):
        """It is READ, never spelled: a plane on `hotkey = "F1"` must not be told about a
        key that does nothing. The exemption must not become a hardcoded string."""
        state.record_density("f-min3", "minimal")
        with mock.patch("charter.statusline._alerts", return_value=[]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=7), \
             mock.patch.dict(config.FRAME, {"hotkey": "F1"}):
            out = tui.strip_ansi(slots.render("bottom", "f-min3"))
        self.assertEqual(out, "7 todos · F1 palette")

    def test_a_starved_pane_still_drops_it(self):
        """An exemption from the DENSITY, never from the arithmetic. A hint sliced in half
        is the false-clean failure `_fit_fields` exists to refuse, so a pane with room for
        one field gets one field — which is width doing its job, not `minimal` doing it.
        """
        state.record_density("f-min4", "minimal")
        with mock.patch("charter.statusline._alerts", return_value=[]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=7), \
             mock.patch.dict(config.FRAME, {"hotkey": "F2"}), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((9, 3))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = tui.strip_ansi(slots.render("bottom", "f-min4"))
        self.assertEqual(out, "7 todos")

    def test_a_normal_density_is_unchanged(self):
        """The control. Nothing about this is a new field or a new order — `normal` drew
        both already, and an exemption that changed what a full frame says would be a
        second defect wearing the first one's fix."""
        self.assertEqual(self._row("f-normal"), "7 todos · F2 palette")


class FitFields(unittest.TestCase):
    """`slots._fit_fields` in isolation, free of `_bottom`'s other, always-present
    fields (todo, hotkey) — so a test can construct the exact width pressure it wants
    without those competing for the same budget."""

    def test_the_first_field_is_kept_even_when_it_alone_exceeds_the_width(self):
        self.assertEqual(slots._fit_fields([("a", "AAAAAAAAAA")], 3), {"a"})

    def test_a_later_field_is_dropped_whole_once_the_budget_runs_out(self):
        self.assertEqual(
            slots._fit_fields([("a", "AAA"), ("b", "BBBBBBBBBB")], 6), {"a"})

    def test_an_exempt_field_is_not_counted_by_the_limit_and_not_stopped_by_it(self):
        """#743's mechanism, in isolation. `limit=1` is what a `terse` density asks for;
        an exempt name neither consumes that one slot nor is cut off by it — and it is
        reached even though it is LAST in the priority order, which is where the hotkey
        hint sits and why the loop `continue`s past a capped field instead of breaking."""
        fields = [("alert", "AAA"), ("todo", "TTT"), ("hotkey", "HHH")]
        self.assertEqual(slots._fit_fields(fields, 80, limit=1), {"alert"})
        self.assertEqual(
            slots._fit_fields(fields, 80, limit=1, exempt=frozenset({"hotkey"})),
            {"alert", "hotkey"})

    def test_an_exempt_field_does_not_spend_the_densitys_one_slot(self):
        """Asked with the exempt field FIRST, which `_bottom` does not do today and which
        is exactly why it is worth asking here. `hotkey` is last in that caller's priority
        order, so an implementation that counted an exempt field against the cap would
        behave identically on the only list charter passes it — and would silently eat the
        one slot the news fields get the day a second piece of chrome is added above them.
        A general function's contract does not get to depend on its one current caller.
        """
        fields = [("hotkey", "HHH"), ("alert", "AAA"), ("todo", "TTT")]
        self.assertEqual(
            slots._fit_fields(fields, 80, limit=1, exempt=frozenset({"hotkey"})),
            {"hotkey", "alert"})

    def test_an_exempt_field_still_answers_to_the_width(self):
        """The half that keeps this an exemption from the density rather than from the
        arithmetic: no budget, no field, exempt or not."""
        fields = [("todo", "TTTTTTTT"), ("hotkey", "HHHHHHHH")]
        self.assertEqual(
            slots._fit_fields(fields, 10, limit=1, exempt=frozenset({"hotkey"})),
            {"todo"})

    def test_the_row_names_the_hotkey_hint_and_nothing_else(self):
        """`_ALWAYS` is a set so that "which fields are chrome rather than news" is a
        question with an answer somebody can read, and so the next field of that kind
        joins a list rather than an `or` buried in a loop. One entry today."""
        self.assertEqual(slots._ALWAYS, frozenset({"hotkey"}))

    def test_measures_in_display_cells_not_characters(self):
        """`"測"` is one character but TWO display cells. Sized so a `len()`-based
        mistake (which would compute 1, leaving 4 of the 5-wide budget spare after
        `"A"`) says there is room for it, while the real width-based measurement
        (2, against a `budget - sep` of only... the arithmetic is spelled out in the
        comment below) correctly says there is not. Unlike `_row`, nothing here ever
        slices a field's TEXT, so a trailing safety-net truncate can never paper over
        this mistake the way it did for the predecessor's CJK repo-name test — this
        asserts on `_fit_fields`'s own return value, before any string is even
        assembled.

        budget=5: `"A"` costs 1 -> 4 left. `"測"` needs `width("測")=2 + sep(3) = 5`
        against that `4` -> doesn't fit, correctly dropped. A `len()`-based version
        would compute `len("測")=1 + 3 = 4`, which DOES fit against `4` -> wrongly
        kept.
        """
        self.assertEqual(slots._fit_fields([("alert", "A"), ("news", "測")], 5),
                         {"alert"})
        # One column more and the real (width-based) arithmetic agrees it fits too —
        # confirms this is a boundary case, not `_fit_fields` refusing CJK outright.
        self.assertEqual(slots._fit_fields([("alert", "A"), ("news", "測")], 6),
                         {"alert", "news"})


class ReposTable(PersonaIso, unittest.TestCase):
    """#488's repo table, in the bordered pane of its own #515 gave it.

    The rows are `statusline.py`'s OWN wide table — same four columns, same declared
    widths, same markers and CI glyphs — composed straight from `gather`'s cache: never a
    `git` call, never `glstate`, and never a repo directory (see `_table_row`'s docstring
    for the one column that costs a filesystem walk per row and is therefore absent).

    Every test here renders through the real `slots.render("repos", …)` rather than
    calling `_table_lines` directly, because what #488 actually promises is that a
    PANEL shows this — a helper returning perfect rows that `_repos` never asks for
    would satisfy a unit test of the helper and none of the promise.
    """

    def setUp(self):
        super().setUp()
        # **The workspace these frames draw, on disk** — and it is fixture, not decoration
        # (#752). Every "nothing here" sentence this pane can say is a claim about a
        # workspace that EXISTS: `⋯ gathering this workspace's repos…` and `no clones in
        # <ws>` are both waiting on something, and a workspace that is not there is not
        # waiting on anything, so it now gets a line of its own instead. A fixture with no
        # directory was asking this pane about a plane where neither sentence is the true
        # answer, and the assertions below are about the ones where they are.
        (config.WORKSPACES_DIR / config.DEFAULT_WORKSPACE).mkdir(parents=True,
                                                                 exist_ok=True)

    def _render(self, fid="f-1", *, cols=200, rows=24) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render("repos", fid)

    def test_lists_a_repo_from_the_cache(self):
        _seed("f-1", repos=[_row("demo")])
        self.assertIn("demo", self._render())

    def test_the_pane_is_headed_the_way_the_sidebars_sections_are(self):
        """#515 gave the table a bordered pane; #516 had just given the sidebar's
        sections headings. An unlabelled box of tree rows beside a labelled one reads as
        an overflow of its neighbour, which is the impression this issue exists to
        remove — and the table's first row is `├─`, a glyph that means "there is more
        above me" and had nothing above it once the attention row moved out.

        Composed through `_sidebar_head`, the same helper `_right` uses, rather than a
        string of its own: two components labelled two ways is the drift that helper was
        extracted to stop. The COUNT is asserted separately from the word, because a
        heading that says `repos` and lies about how many there are is worse than none.
        """
        _seed("f-1", repos=[_row("demo"), _row("other")])
        first = tui.strip_ansi(self._render().split("\n")[0])
        self.assertEqual(first, tui.strip_ansi(slots._sidebar_head("repos", 2, 200)))
        self.assertIn("repos", first)
        self.assertIn("2", first)

    def test_the_two_lines_that_are_not_a_table_carry_no_heading(self):
        """`▪ repos 0` above "no clones in demo" is the same fact twice in a two-row
        pane, and above "gathering…" it is a count charter does not have yet. Both of
        those panes are one line, and the line is the sentence."""
        self.assertEqual(len(self._render("f-never-gathered").split("\n")), 1)
        _seed("f-known-empty")
        empty = self._render("f-known-empty")
        self.assertEqual(len(empty.split("\n")), 1)
        self.assertNotIn("repos", tui.strip_ansi(empty))

    def test_the_table_starts_under_the_heading_with_no_attention_row_in_this_pane(self):
        """#515's non-negotiable, and the inverse of the one #488 needed. The attention
        row is `bottom`'s own pane now, so this pane is the table and NOTHING else: a
        renderer that kept composing the row here would put a second copy of the todo
        count, the alert and the hotkey hint on screen, one pane above the real one.

        Asserted as line 0 specifically — a table drawn under a leftover row would still
        contain "demo" and satisfy a membership check. The `bottom` renderer is asked in
        the same test so the pair is pinned together: exactly one of the two panes draws
        the row, and it is not this one."""
        _seed("f-1", repos=[_row("demo")])
        out = self._render()
        self.assertIn("demo", tui.strip_ansi(out.split("\n")[1]))
        self.assertNotIn("todo", tui.strip_ansi(out))
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((200, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            row = slots.render("bottom", "f-1")
        self.assertIn("todo", tui.strip_ansi(row))
        self.assertNotIn("demo", tui.strip_ansi(row))

    def test_a_plane_with_no_repos_says_so_rather_than_drawing_an_empty_pane(self):
        """The floor `layout.repos_rows` keeps, seen from the renderer's side — and what
        #515 changed about it. While this table shared `bottom` with the attention row, a
        workspace with 0 clones simply produced no table rows and absence said it. In a
        bordered pane of its own, absence is an empty rectangle, which reads as a table
        that failed to draw rather than as a workspace with nothing in it.

        So the pane is one row and that row is a sentence — with the command that changes
        it, the shape every `statusline._alerts` row already has. Both halves asserted:
        one row (a renderer padding the pane would be red) and the workspace named in it
        (a renderer that said something generic would not carry the fix).

        `_seed` is what makes this the honest case rather than the unknown one below: a
        cache exists and it says zero repos. Delete the seed and this is a DIFFERENT
        claim, which is #512."""
        _seed("f-1")
        out = self._render()
        self.assertEqual(len(out.split("\n")), 1, out)
        self.assertIn("no clones", tui.strip_ansi(out))
        self.assertIn("charter clone", tui.strip_ansi(out))

    def test_a_workspace_name_the_rungs_never_checked_is_still_one_line(self):
        """A workspace name with no `contain.one_line` over it, and the reason has to be
        the true one. It is NOT that every rung of `state.workspace_for` name-checks its
        answer: rung 0 does (`valid_name`), but the last rung is `workspace.resolve()`,
        which returns `$CHARTER_WORKSPACE` stripped and otherwise untouched — so a name
        with a newline in it reaches this renderer verbatim, as this test's first
        assertion measures.

        What contains it is `tui.truncate`, which runs `tui.sanitize` first: the newline
        is not charter's markup, so the pane still draws exactly ONE line — the property
        the whole slot is built on, since a `repos` pane that quietly became three rows
        tall would push the attention strip off the bottom of the window.

        **The line it lands on is `_gone_lines` since #752, and that is the whole point of
        naming it here.** A name no rung checked cannot name a workspace that EXISTS —
        `workspace.exists` asks `valid_name` before it asks the filesystem, precisely
        because `workspaces/..` is a real directory — so this state reaches the "not on
        disk" sentence and can no longer reach `_empty_lines` at all. That makes
        `_gone_lines` the one line in this pane that is interpolated with a value nobody
        has checked, and the one whose `tui.truncate` is load-bearing for reasons of
        CONTENT rather than of length.

        The hostile value is asserted to have got through as well as to have been
        contained. Asserting containment alone would pass just as well on a build where a
        rung DID reject it and the sentence said `default` — a test that proves nothing
        about the line it is named for."""
        hostile = "ev\nil\x1b[31m;rm -rf /"
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": hostile}, clear=True):
            self.assertEqual(state.workspace_for("f-known-empty"), hostile)
            _seed("f-known-empty")
            out = self._render("f-known-empty")
        self.assertEqual(len(out.split("\n")), 1, repr(out))
        self.assertIn("rm -rf /", tui.strip_ansi(out))
        self.assertIn("no workspace", tui.strip_ansi(out))

    def test_a_workspace_name_longer_than_the_pane_is_cut_to_it(self):
        """`_empty_lines`' own bound, measured with a name the pane cannot hold.

        Since #752 this line is only ever reached with a name `workspace.exists` accepted,
        so the `tui.sanitize` half of its `tui.truncate` has nothing left to do — the
        alphabet `instance.WORKSPACE_NAME_RE` allows holds no newline and no ESC. What is
        NOT bounded by that check is LENGTH: a workspace name is arbitrarily long, and an
        untruncated line here is a `repos` pane wider than its own pane, which wraps and
        becomes two rows. Measured at `statusline._LEFT_W`, the narrowest width this pane
        draws anything at all at, with a name long enough that the unbounded version
        overflows — the bound is red the moment the truncate goes."""
        long = "w" * 90
        (config.WORKSPACES_DIR / long).mkdir(parents=True, exist_ok=True)
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": long}, clear=True):
            self.assertEqual(state.workspace_for("f-long-empty"), long)
            _seed("f-long-empty")
            out = self._render("f-long-empty", cols=statusline._LEFT_W)
        self.assertEqual(len(out.split("\n")), 1, repr(out))
        self.assertIn("no clones", tui.strip_ansi(out))
        self.assertLessEqual(tui.width(tui.strip_ansi(out)), statusline._LEFT_W, out)

    def test_a_frame_whose_repos_are_not_gathered_yet_says_so_rather_than_showing_none(self):
        """#512, at the renderer. `cmd_launch` deletes the cache before it draws anything
        and a detached child fills it a beat later, so there is a real window in which
        charter does not KNOW this frame's repos — and drawing that as an empty table is
        the same confidently-wrong output the `left` sidebar was retired for in #488.

        The two cases are asserted against each other rather than in isolation, because
        "says something" and "says nothing" are only meaningful as a pair: a renderer that
        drew this line unconditionally would pass a membership check on its own and would
        be a permanent lie on every plane with no clones."""
        unknown = self._render("f-never-gathered")
        _seed("f-known-empty")
        empty = self._render("f-known-empty")
        self.assertIn("gathering", unknown)
        self.assertEqual(len(unknown.split("\n")), 1)
        self.assertNotIn("gathering", empty)
        self.assertIn("no clones", tui.strip_ansi(empty))
        self.assertEqual(len(empty.split("\n")), 1)

    def test_the_gathering_line_goes_the_moment_the_rows_arrive(self):
        """It is a statement about now, not furniture. Same frame id, one cache write
        between the two renders — the sequence a real launch actually performs."""
        self.assertIn("gathering", self._render("f-1"))
        _seed("f-1", repos=[_row("demo")])
        after = self._render("f-1")
        self.assertNotIn("gathering", after)
        self.assertIn("demo", after)

    def test_a_pane_too_narrow_for_the_table_says_nothing_either(self):
        """`_table_cap` answers 0 below `statusline._LEFT_W` and `_repos` composes nothing
        on a budget of 0, so a "gathering" line cannot appear where a table never will:
        one that did would show up while the rows were unknown and VANISH once they were
        known — "the repos went away", which is worse than the silence it replaces.

        Asserted through the whole renderer at both widths in one test, because what is
        being pinned is that the two agree: the SAME frame, gathered or not, draws the
        SAME line at 90 columns, and it is not the gathering one.

        And the pane is not even SPLIT at that width by a launch since #515
        (`layout.visible_slots`), which is asserted here beside the renderer rather than
        only over in the layout tests: the two together are what stop a blank bordered
        rectangle appearing. The line the renderer does draw is for the pane a RESIZE
        leaves behind — `cmd_resize` re-sizes panes and never destroys them."""
        before = self._render("f-never-gathered", cols=90)
        _seed("f-never-gathered", repos=[_row("demo")])
        after = self._render("f-never-gathered", cols=90)
        self.assertEqual(before, after)
        self.assertNotIn("gathering", before)
        self.assertIn("too narrow", tui.strip_ansi(before))
        self.assertEqual(len(before.split("\n")), 1, before)
        self.assertNotIn("repos", layout.visible_slots(
            ["top", "bottom", "repos", "right"], 90, 40, 100, 20))

    def test_a_repaint_never_runs_a_gather_of_its_own(self):
        """The rule `_table_lines`' own docstring states and #512 found broken: a panel
        reads the cache and nothing else. `gather.read` falls back to a live `scan()`
        when there is no cache — three git invocations and ~35ms — and `cmd_launch`
        guarantees exactly that state at launch, so every repaint of a fresh frame was
        paying for one, on the ONE animated slot (`slots.ANIMATED`), five times a second
        for as long as anything was in flight.

        Asserted on the unknown case specifically: with a cache present, a renderer that
        called `scan` unconditionally would still be caught, but a renderer that called it
        only when there is nothing to read — the actual defect — would not."""
        with mock.patch.object(gather, "scan",
                               side_effect=AssertionError("a repaint gathered")):
            self.assertIn("gathering", self._render("f-never-gathered"))

    def test_a_dirty_repo_shows_the_dirty_marker(self):
        _seed("f-1", repos=[_row("demo", dirty=True)])
        self.assertIn("*", tui.strip_ansi(self._render()))

    def test_a_clean_repo_shows_no_dirty_marker(self):
        _seed("f-1", repos=[_row("demo", dirty=False)])
        self.assertNotIn("*", tui.strip_ansi(self._render()))

    def test_an_open_change_shows_its_sigil_and_number(self):
        _seed("f-1", repos=[_row("demo", change=42, sigil="!")])
        self.assertIn("!42", tui.strip_ansi(self._render()))

    def test_a_failing_ci_status_shows_its_glyph(self):
        _seed("f-1", repos=[_row("demo", ci="failed")])
        self.assertIn("✗", tui.strip_ansi(self._render()))

    def test_the_wide_tables_own_ci_label_survives_now_there_is_room_for_it(self):
        """The gain #488 is actually for. `left` had 22 columns and could show only the
        glyph; the wide table has `_CI_W` and shows `✗ failed` — the same
        `statusline._ci_part` the status line calls, which is why this reads identically
        in both surfaces. A frame drawing the glyph alone would still pass the test above
        and would still be the downgrade the issue is about."""
        _seed("f-1", repos=[_row("demo", ci="failed")])
        self.assertIn("failed", tui.strip_ansi(self._render()))

    def test_the_branch_gets_the_wide_tables_own_column_not_a_22_column_squeeze(self):
        """`left`'s own docstring conceded it could not do this: `_NAME_W` (32) and
        `_BRANCH_W` (34) alone exceed a 22-column pane, so a real branch name was always
        elided. This is the reviewer's own repro fixture from #385's fix round 1, which
        rendered as `charter worktree-reca…` there and must render whole here."""
        _seed("f-1", repos=[_row("charter", branch="worktree-recall-since",
                                 dirty=True, ahead=1, ci="failed")])
        out = tui.strip_ansi(self._render())
        self.assertIn("worktree-recall-since", out)
        self.assertIn("*", out, "the dirty marker must survive")
        self.assertIn("✗", out, "the CI glyph must survive")

    def test_a_piece_from_the_worktrees_cache_field_is_shown(self):
        _seed("f-1", repos=[_row("demo")],
              worktrees=[_row("piece-one", repo="demo")])
        self.assertIn("piece-one", tui.strip_ansi(self._render()))

    def test_a_pieces_branch_column_is_emptied_when_it_restates_its_own_name(self):
        """`charter worktree add <repo> <piece>` names the branch after the piece, so by
        default those two columns print the same word twice — `statusline._repo_rows`
        empties the branch cell when they agree, and this table does the same rather than
        spending 34 columns restating the name beside it. The markers still render: dirty
        is true of the tree whatever its branch is called."""
        _seed("f-1", repos=[_row("demo", worktree_count=1)],
              worktrees=[_row("same-name", repo="demo", branch="same-name",
                              dirty=True)])
        piece = tui.strip_ansi(self._render().split("\n")[-1])
        self.assertEqual(piece.count("same-name"), 1, piece)
        self.assertIn("*", piece, "the dirty marker survives an emptied branch cell")

    def test_a_multi_repo_workspaces_piece_count_shows_as_a_badge(self):
        """`data["worktrees"]` is `[]` here (as it always is with two repos —
        `gather._detail_worktrees`' own single-repo rule), so the badge is the ONLY way
        either repo's pieces are visible at all."""
        _seed("f-1", repos=[_row("demo", worktree_count=3),
                            _row("second", worktree_count=0)],
              worktrees=[])
        self.assertIn("⑂3", tui.strip_ansi(self._render()))

    def test_a_repo_whose_pieces_are_all_shown_as_rows_carries_no_badge(self):
        """The single-repo case: every piece already has its own row, so the badge —
        "there is more you cannot see" — would be actively misleading if it appeared."""
        _seed("f-1", repos=[_row("demo", worktree_count=1)],
              worktrees=[_row("piece-one", repo="demo")])
        self.assertNotIn("⑂", tui.strip_ansi(self._render()))

    def test_picks_the_dirty_repo_over_clean_ones_when_over_budget(self):
        """`statusline._pick_rows` is CALLED here, not reinvented — the same ranking
        `statusline.py`'s own production regression (an unranked slice of 18 clones
        showed thirteen clean repos and hid the one dirty one) was filed against."""
        clean = [_row(f"clean-{i}") for i in range(statusline._MAX_REPO_LINES)]
        dirty = _row("zzz-dirty-one-past-the-cap", dirty=True)
        _seed("f-1", repos=clean + [dirty])
        self.assertIn("zzz-dirty-one-past-the-cap", tui.strip_ansi(self._render()))

    def test_the_overflow_note_matches_the_wide_tables_own_wording(self):
        """One claim, one wording. `_repo_rows`' own overflow line says `, all clean`,
        and a second surface saying `, clean` for the same claim is a divergence a reader
        has to reconcile."""
        clean = [_row(f"clean-{i}") for i in range(statusline._MAX_REPO_LINES + 1)]
        _seed("f-1", repos=clean)
        out = tui.strip_ansi(self._render(rows=8))
        self.assertIn(", all clean)", out)
        self.assertNotIn("more, clean)", out)

    def test_the_overflow_note_refuses_to_say_all_clean_when_it_is_hiding_trouble(self):
        """The half that matters. `_needs_attention` is what stands between the operator
        and a panel that hides a failing pipeline behind the words "all clean" — the
        false-clean reading this whole surface is built to avoid.

        MORE repos need attention than the pane has rows, so `_pick_rows` ranking them
        all to the front cannot empty the hidden set of them: two rows of table for five
        failing repos means at least three failing ones are hidden, whatever the ranking
        does. A single one would be ranked in and the note would then be telling the
        truth — which is why the sibling test above asserts the `, all clean` wording and
        this one asserts its absence."""
        rows = [_row(f"failing-{i}", ci="failed") for i in range(5)]
        rows += [_row(f"clean-{i}") for i in range(5)]
        _seed("f-1", repos=rows)
        out = tui.strip_ansi(self._render(rows=4))
        self.assertIn("below)", out)
        self.assertNotIn("all clean", out)

    def test_a_one_row_budget_spends_it_on_saying_how_much_is_hidden(self):
        """The narrowest overflow case, and a false-clean one until it was closed. The
        note used to be appended on top of the budget and trimmed off at the end, so a
        pane with room for exactly one table row showed one repo — clean, on main — and
        nothing saying the other nine existed. A pane claiming one clean repo IS the plane
        is the reading this module refuses everywhere else, so the row is spent on the
        note instead: "there is more here than fits" outranks an arbitrary one of them."""
        rows = [_row(f"repo{i}") for i in range(10)]
        rows[3] = _row("the-dirty-one", dirty=True)
        _seed("f-1", repos=rows)
        # Split BEFORE stripping: `tui.strip_ansi` runs `tui.sanitize`, which replaces a
        # newline like any other control character, so stripping the whole render first
        # would fold every row onto one line and make a line-count assertion meaningless.
        lines = [tui.strip_ansi(ln) for ln in self._render(rows=2).split("\n")]
        self.assertEqual(len(lines), 2, lines)
        self.assertIn("10 below", lines[1])
        self.assertNotIn("all clean", lines[1],
                         "it is hiding a dirty repo and must not claim otherwise")

    def test_the_table_is_bounded_by_the_panes_own_measured_height(self):
        """The renderer must spend the pane it HAS, not the one the launcher intended:
        a resize changes the pane under a running panel and nothing bumps the frame's
        version for it. Asserted at two heights against the same cache, so a renderer
        ignoring the measurement and emitting everything is red at the short one."""
        _seed("f-1", repos=[_row(f"repo{i}") for i in range(10)])
        self.assertEqual(len(self._render(rows=6).split("\n")), 6)
        self.assertEqual(len(self._render(rows=20).split("\n")), 1 + 10)

    def test_the_height_the_launcher_asks_for_is_the_height_the_renderer_fills(self):
        """The seam #488 turns on, over every input that changes either side's answer.

        `slots.repos_rows_wanted` is what tells `layout.slot_sizes` how tall to split the
        pane; `_repos` is what fills it. If the two disagreed, every frame would come up
        either padded with blank rows the harness could have had, or with a table cut off
        and nothing saying so — and neither is visible from either side alone.

        **The PROPERTY is "the pane the launcher asks for is the pane the panel fills",
        not "the row count matches".** #488 shipped a sizer that read the repo count and
        a renderer that also read the WIDTH (no table below `statusline._LEFT_W`) and the
        DENSITY (`_TERSE_ROWS` at `terse`), so the seam held only at the one shape the
        original test used — wide, `normal`. Every input either side consults is varied
        here: repo count across the cap, width across `_LEFT_W` and down to widths the
        pane is no longer even split at, and every level in `instance.FRAME_DENSITY`. The
        next input to appear — a fourth density, a `repos`-specific `min-cols` — has to be
        added to this loop, and until it is, its own dimension is unpinned rather than
        silently wrong.

        Rendered into a pane of EXACTLY the height the sizer asked for, at EXACTLY the
        width the sizer was asked about, and counted. **`layout.repos_rows` is what turns
        the sizer's answer into a pane height**, so it is called here rather than the raw
        want: a 0 (no clones, or a width with no table in it) becomes the one-row pane
        that says so, and asserting against the raw number would demand a zero-line render
        the pane could not hold anyway.
        """
        for level in (None, *sorted(instance.FRAME_DENSITY)):
            for n in (0, 1, 4, 9, statusline._MAX_REPO_LINES + 3):
                for cols in (50, 80, statusline._LEFT_W - 1, statusline._LEFT_W, 200):
                    with self.subTest(density=level, repos=n, cols=cols):
                        fid = f"wanted-{level}-{n}-{cols}"
                        _seed(fid, repos=[_row(f"repo{i}") for i in range(n)])
                        if level is not None:
                            state.record_density(fid, level)
                        want = slots.repos_rows_wanted(fid, pane_cols=cols)
                        tall = layout.repos_rows(content_rows=want, window_rows=50,
                                                 slots=["top", "bottom", "repos"])
                        out = self._render(fid, cols=cols, rows=tall)
                        if cols < statusline._LEFT_W:
                            # Not a pane a LAUNCH splits at all any more
                            # (`layout.visible_slots`); one a resize leaves behind draws
                            # the single line saying why, in the single row the floor
                            # gives it.
                            self.assertEqual(tall, 1)
                            self.assertEqual(len(out.split("\n")), 1, out)
                            self.assertIn("too narrow", tui.strip_ansi(out))
                            continue
                        self.assertEqual(len(out.split("\n")), tall, out)

    def test_a_frame_too_narrow_for_the_table_is_sized_for_the_row_it_can_draw(self):
        """The width half of the seam, stated as the number the LAUNCHER hands tmux.

        An 80-column terminal cannot draw this table at all. Sized from the repo count
        alone it was given a pane for the table anyway — six repos meant a seven-row pane
        holding one line, and the other six rows came off the harness. Since #515 the slot
        is dropped at those widths instead of sized, and this is the number that says so:
        zero rows WANTED, which is a different statement from the one-row floor
        `layout.repos_rows` applies to a pane that does get split.

        Asserted against a repo count large enough that the two answers cannot coincide,
        and at `_LEFT_W` itself so the boundary is pinned from both sides rather than
        only from the narrow one."""
        _seed("narrow", repos=[_row(f"repo{i}") for i in range(6)])
        for cols in (50, 80, statusline._LEFT_W - 1):
            with self.subTest(cols=cols):
                self.assertEqual(slots.repos_rows_wanted("narrow", pane_cols=cols), 0)
        self.assertEqual(slots.repos_rows_wanted("narrow",
                                                 pane_cols=statusline._LEFT_W), 1 + 6)

    def test_a_terse_density_asks_for_a_shorter_pane_not_a_blanker_one(self):
        """`minimal` exists to give the harness its rows back — `instance.FRAME_DENSITY`
        says so in as many words. It shipped costing the harness exactly what `normal`
        cost and drawing less in it: the renderer capped the TABLE at `_TERSE_ROWS`, the
        sizer never read the density at all, so ten repos meant an eleven-row pane with
        five lines in it and six blank.

        Both levels asked for the same frame and the same width, so the only difference
        between the two numbers is the one the level is for."""
        _seed("dense", repos=[_row(f"repo{i}") for i in range(10)])
        state.record_density("dense", "normal")
        wide = slots.repos_rows_wanted("dense", pane_cols=200)
        state.record_density("dense", "minimal")
        terse = slots.repos_rows_wanted("dense", pane_cols=200)
        self.assertEqual(wide, 1 + 10)
        self.assertEqual(terse, 1 + slots._TERSE_ROWS)
        self.assertLess(terse, wide)

    def test_a_pane_too_narrow_for_the_table_draws_no_table_rather_than_a_cut_one(self):
        """Every column after the branch sits at a fixed offset past
        `_NAME_W + _BRANCH_W`, so a narrow pane loses the CI glyph and the open change
        off the right-hand end — and a dirty, CI-failing repo then renders as a clean
        `charter  main`. Refusing to draw says "no room to say"; a trimmed row says
        "nothing to say", which is the false-clean failure the plan's Global Constraints
        name. The attention row is unaffected — it budgets its own fields."""
        _seed("f-1", repos=[_row("demo", dirty=True, ci="failed")])
        narrow = tui.strip_ansi(self._render(cols=statusline._LEFT_W - 1))
        self.assertNotIn("demo", narrow)
        self.assertIn("too narrow", narrow)
        self.assertIn(str(statusline._LEFT_W), narrow,
                      "the line must say how wide the pane has to be")
        # ...and one column wider, it draws.
        self.assertIn("demo", self._render(cols=statusline._LEFT_W))

    def test_no_line_ever_exceeds_the_panes_width(self):
        _seed("f-1", repos=[_row("a-repo-with-quite-a-long-descriptive-name",
                                 branch="a-very-long-branch-name-that-keeps-going",
                                 change=999999, ci="failed", ahead=12, behind=34)],
              worktrees=[])
        for cols in (statusline._LEFT_W, 120, 200):
            out = self._render(cols=cols)
            for line in out.split("\n"):
                with self.subTest(cols=cols, line=line):
                    self.assertLessEqual(tui.width(line), cols)

    def test_a_cjk_heavy_repo_name_does_not_push_the_table_past_the_pane(self):
        """`tui.Cell` pads and truncates in display CELLS, not characters — a name of 30
        CJK characters is 60 cells and would blow a 37-column name column out by 23 if
        anything here counted characters."""
        cjk = "測" * 30
        _seed("f-1", repos=[_row(cjk, dirty=True, ci="failed")])
        out = self._render(cols=statusline._LEFT_W)
        for line in out.split("\n"):
            with self.subTest(line=line):
                self.assertLessEqual(tui.width(line), statusline._LEFT_W)
        self.assertIn("✗", tui.strip_ansi(out), "the CI cell keeps its own column")

    def test_it_never_reaches_the_wide_tables_own_filesystem_walking_composer(self):
        """#387 pinned a panel's idle tick at exactly one `stat`, and `bottom` is the ONE
        animated slot — at `panel.TICK` a table that walked a directory per row would pay
        that back fourteen times over, five times a second, for the length of every
        dispatch.

        `statusline._tree_cells` is the composer this table deliberately does NOT call:
        it ends in `_presence_for_dir`, a `worktree.locate`/`workspace.clone_of` pair,
        per row. `worktree.dirs_for` is the other one — `_repo_rows` calls it per repo
        for its `⑂N` badge, and the frame reads `worktree_count` out of the cache
        instead. All three are made to raise, so a call is a loud failure rather than a
        slow success.

        Scoped to the three helpers rather than to "any filesystem call": `_bottom`'s
        attention row legitimately resolves the workspace (which reaches
        `worktree.locate` on its own), and the sibling test below is what bounds the
        table's own syscalls."""
        _seed("f-1", repos=[_row("demo", dirty=True, ci="failed", change=3, sigil="!",
                                 worktree_count=2)],
              worktrees=[_row("piece", repo="demo")])
        boom = AssertionError("the table used the wide table's own row composer")
        with mock.patch("charter.statusline._tree_cells", side_effect=boom), \
             mock.patch("charter.statusline._presence_for_dir", side_effect=boom), \
             mock.patch("charter.worktree.dirs_for", side_effect=boom):
            out = tui.strip_ansi(self._render())
        self.assertIn("demo", out)
        self.assertIn("piece", out)
        self.assertIn("⑂2", out, "the badge came from the cache, not a directory walk")

    def test_composing_the_table_opens_no_file_and_starts_no_process(self):
        """The syscall half of the same property, counted rather than timed (a wall-clock
        assertion on a shared CI box measures the box) and counted at the BOTTOM —
        `os.stat`, `builtins.open`, `subprocess.run` — which is where every higher-level
        spelling (`Path.stat`, `Path.exists`, `read_text`, `glob`) must eventually
        arrive. `_table_lines` is handed the cache it would otherwise read, so what is
        left to count is composition alone: fourteen rows of it must cost nothing."""
        import builtins
        import subprocess as _sp
        data = {"gathered_at": 0.0, "workspace": "w", "current_repo": "r0",
                "repos": [_row(f"r{i}", dirty=True, ci="failed", change=i, sigil="!",
                               worktree_count=2) for i in range(14)],
                "worktrees": []}
        real_stat, real_open, real_run = os.stat, builtins.open, _sp.run
        stats, opens, runs = [], [], []
        with mock.patch("os.stat", lambda *a, **k: (stats.append(a),
                                                    real_stat(*a, **k))[1]), \
             mock.patch("builtins.open", lambda *a, **k: (opens.append(a),
                                                          real_open(*a, **k))[1]), \
             mock.patch("subprocess.run", lambda *a, **k: (runs.append(a),
                                                           real_run(*a, **k))[1]):
            lines = slots._table_lines(data, 200, 14)
        self.assertEqual(len(lines), 14)
        self.assertEqual(stats, [], "composing the table stat'ed something")
        self.assertEqual(opens, [], "composing the table opened a file")
        self.assertEqual(runs, [], "composing the table started a process")

    def test_a_taller_pane_costs_the_same_syscalls_as_a_one_row_one(self):
        """#387 pinned a panel's idle tick at exactly one `stat`, and #488 made `bottom`
        the tall slot AND the one animated slot — so the question that budget does not
        answer on its own is whether a REPAINT scales with the pane's height. At
        `panel.TICK` a repaint that cost a syscall per row would pay fourteen times over,
        five times a second, for the length of every dispatch.

        The sibling test above bounds `_table_lines` in isolation, handed a dict; this
        one bounds the whole `slots.render("bottom", …)` a panel actually calls — the
        gather read, the workspace resolve, the alerts and the todo count included — and
        it bounds it DIFFERENTIALLY. An absolute number would have to be revised every
        time the attention row learns a new field, and a revised number is not a budget.
        The claim is that the count does not depend on how many rows are drawn, so the
        same count at one repo and at `_MAX_REPO_LINES` is exactly the claim.

        Primed first: `workspace.resolve` and friends memoise, so the first render of the
        process pays for caches every later one reuses, and comparing an unprimed render
        against a primed one would measure the priming.

        **Counted at every spelling a walk could arrive by, not at `os.stat` alone.** The
        thing being kept out is "touch the filesystem once per row", and a directory walk
        does not spell itself `stat`: `Path.iterdir` is `os.scandir`, `Path.resolve` is
        `os.lstat`, `Path.read_text` is `io.open` and not `builtins.open`, and a `git`
        call is `subprocess.Popen` under `run`. Each of those is the next spelling this
        budget would otherwise miss, so each is counted.

        Measured on this branch: the same calls, in the same order, for a one-row
        repaint and a fourteen-row one."""
        import builtins
        import io
        import subprocess as _sp
        watched = {"os": (os, ("stat", "lstat", "scandir", "listdir")),
                   "io": (io, ("open",)), "builtins": (builtins, ("open",)),
                   "subprocess": (_sp, ("run", "Popen"))}

        def _count(fid, n):
            _seed(fid, current_repo="r0",
                  repos=[_row(f"r{i}", dirty=True, ci="failed", change=i, sigil="!",
                              worktree_count=2) for i in range(n)])
            self._render(fid, cols=200, rows=1 + n)          # prime
            seen: list[str] = []
            patches = []
            for mod_name, (mod, fns) in watched.items():
                for fn in fns:
                    real = getattr(mod, fn)
                    tag = f"{mod_name}.{fn}"
                    patches.append(mock.patch(
                        tag,
                        (lambda r, t: lambda *a, **k: (seen.append(t), r(*a, **k))[1])(
                            real, tag)))
            with contextlib.ExitStack() as stack:
                for p in patches:
                    stack.enter_context(p)
                out = self._render(fid, cols=200, rows=1 + n)
            return len(out.split("\n")), seen

        short_lines, short = _count("cost-1", 1)
        tall_lines, tall = _count("cost-many", statusline._MAX_REPO_LINES)
        self.assertEqual(short_lines, 1 + 1)
        self.assertEqual(tall_lines, 1 + statusline._MAX_REPO_LINES,
                         "the tall render drew no more rows — this proves nothing")
        self.assertTrue(short, "nothing was counted at all — the budget is vacuous")
        self.assertEqual(sorted(tall), sorted(short),
                         f"a {tall_lines}-row repaint cost {len(tall)} filesystem calls "
                         f"where a {short_lines}-row one cost {len(short)} — the table "
                         f"is doing per-row filesystem work")
        self.assertNotIn("subprocess.run", tall, "a repaint started a process")
        self.assertNotIn("subprocess.Popen", tall, "a repaint started a process")

    def test_a_failing_gather_read_yields_a_line_rather_than_an_exception(self):
        """A panel that raises leaves a hole in the frame — `slots.render`'s own promise,
        pinned against a renderer that reaches into a real dependency.

        Rendered at a width the table is actually attempted at: below
        `statusline._LEFT_W` there is no table, so the cache is never reached and the
        test would pass by never running the code it claims to bound.

        **`cached`, not `read` (#512).** `_repos` stopped calling `gather.read` when a
        panel stopped being allowed to fall back to a live `scan()`, and this test kept
        passing against a mock nothing called any more — the exact "instrumenting one
        function while claiming to bound a property" shape. It is pinned to whichever
        function the renderer actually reaches, and `_seed` is deliberately NOT called
        first: a cache on disk would make a raising reader unreachable a second way."""
        with mock.patch.object(gather, "cached", side_effect=RuntimeError("boom")), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((200, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            self.assertIn("charter", slots.render("repos", "f-1"))


class ReposRowsWanted(PersonaIso, unittest.TestCase):
    """`slots.repos_rows_wanted` — the number the LAUNCHER sizes the table pane from."""

    def test_a_plane_with_no_repos_wants_no_table_rows_at_all(self):
        _seed("f-1")
        self.assertEqual(slots.repos_rows_wanted("f-1", pane_cols=200), 0)

    def test_it_grows_with_the_repos_and_the_pieces_alike(self):
        _seed("f-1", repos=[_row("a"), _row("b")],
              worktrees=[_row("p", repo="a")])
        self.assertEqual(slots.repos_rows_wanted("f-1", pane_cols=200), 1 + 3)

    def test_it_is_capped_at_the_wide_tables_own_row_budget(self):
        """A workspace with forty clones must ask for a fourteen-row pane, not a
        forty-row one — `_MAX_REPO_LINES` is the same total-row budget the wide table
        keeps, reused rather than invented fresh."""
        _seed("f-1", repos=[_row(f"r{i}") for i in range(40)])
        self.assertEqual(slots.repos_rows_wanted("f-1", pane_cols=200),
                         1 + statusline._MAX_REPO_LINES)

    def test_the_width_is_required_and_cannot_be_confused_with_the_row_count(self):
        """Keyword-only, so the launcher cannot hand it a window HEIGHT and get a
        plausible-looking number back. #500's defect was a caller that had the width and
        did not pass it (`cmd_resize` measured it into `_cols`); a positional parameter
        would have turned that into the next one silently passing the wrong measurement.
        A missing width is a `TypeError` at the call site, not a default of "wide"."""
        _seed("f-1", repos=[_row("a")])
        with self.assertRaises(TypeError):
            slots.repos_rows_wanted("f-1")
        with self.assertRaises(TypeError):
            slots.repos_rows_wanted("f-1", 200)

    def test_the_width_asked_for_is_the_panes_and_a_window_width_will_not_fit(self):
        """The rename is the guard, and this is what pins it. Round 2 of #500 spelled this
        argument `cols` and every caller handed it the WINDOW's width — right only when
        nothing vertical was split before `bottom`. `pane_cols` is not a nicer name for
        the same number: it is the name that makes the old call a `TypeError` here rather
        than six blank rows in a frame nobody thought to re-measure.

        Asserted on the signature rather than only on a failing keyword, so a future
        `**kwargs` (or a `cols` alias added back for compatibility) is red too — an alias
        would restore exactly the confusion this closes. The next spelling to refuse is
        `width`, which is what `_table_cap` calls its own parameter: it takes the pane's
        width because the RENDERER measures a pane, and a caller that copies that name up
        here would be naming the thing correctly by luck rather than by contract."""
        import inspect
        sig = inspect.signature(slots.repos_rows_wanted)
        self.assertEqual([p.name for p in sig.parameters.values()], ["fid", "pane_cols"])
        self.assertEqual(sig.parameters["pane_cols"].kind,
                         inspect.Parameter.KEYWORD_ONLY)
        _seed("f-1", repos=[_row("a")])
        with self.assertRaises(TypeError):
            slots.repos_rows_wanted("f-1", cols=200)

    def test_it_never_runs_a_git_sweep(self):
        """It is called on a launch the operator is waiting on, and again on every step
        of a terminal drag. `gather.row_count` answers from the cache when there is one
        and from a directory listing when there is not; a `scan()` on either path would
        put a `git status` per repo inside a `window-resized` hook."""
        _seed("f-1", repos=[_row("a")])
        with mock.patch("charter.frame.gather.scan",
                        side_effect=AssertionError("row_count ran a scan")):
            self.assertEqual(slots.repos_rows_wanted("f-1", pane_cols=200), 1 + 1)

    def test_a_narrow_frame_does_not_even_ask_how_many_repos_there_are(self):
        """The launch path reaches `gather.row_count` with no cache by design
        (`cmd_launch` calls `gather.discard` first), where it costs a directory listing.
        Below `statusline._LEFT_W` the answer is no table at all whatever the count is, so
        the listing is work with no reader — and `cmd_resize` would pay it again on every
        step of a drag that is narrowing the terminal.

        `row_count` itself is made to raise, so an implementation that asks and then
        discards is a loud failure rather than a slow success."""
        _seed("f-1", repos=[_row(f"r{i}") for i in range(6)])
        with mock.patch("charter.frame.gather.row_count",
                        side_effect=AssertionError("counted rows it had no room for")):
            self.assertEqual(slots.repos_rows_wanted("f-1", pane_cols=80), 0)


class RightRenderer(PersonaIso, unittest.TestCase):
    """`right`: `statusline._persona_chip_cells` called, not reassembled — each chip
    already carries its own memory badge, in-flight badge and vault dot."""

    def test_lists_a_persona_chip(self):
        self.make_persona("alice")
        self.assertIn("alice", tui.strip_ansi(slots.render("right", "f-1")))

    def test_degrades_to_a_readable_line_with_no_personas(self):
        out = slots.render("right", "f-1")
        self.assertTrue(out.strip())
        self.assertIn("no personas", tui.strip_ansi(out))

    def test_calls_persona_chips_rather_than_reassembling_it(self):
        """A fix to a chip (its vault dot, its memory badge, its in-flight badge)
        must land here the moment it lands in the status line — pinned by handing
        `_persona_chip_cells` values nothing in `_right` could have produced on its
        own, and requiring BOTH halves survive to the pane byte-for-byte.

        Both halves, because #516 is exactly the change that could have lost one: the
        badges now go into a column of their own, and a `_right` that recomposed them
        out of `_mem_badge`/`_health_mark`/`_inflight_badge` itself would still print a
        persona's name."""
        with mock.patch("charter.statusline._persona_chip_cells",
                        return_value=[statusline.PersonaChip(
                            "alice", "SENTINEL-HEAD-0xF00D", "SENTINEL-BADGE-0xBEEF")]):
            out = slots.render("right", "f-1")
        self.assertIn("SENTINEL-HEAD-0xF00D", out)
        self.assertIn("SENTINEL-BADGE-0xBEEF", out)

    def test_never_exceeds_the_pane_width(self):
        self.make_persona("a-persona-with-quite-a-long-descriptive-name")
        with mock.patch("os.get_terminal_size", return_value=os.terminal_size((22, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("right", "f-1")
        for line in out.splitlines():
            with self.subTest(line=line):
                self.assertLessEqual(tui.width(line), 22)

    def test_a_cjk_heavy_persona_name_still_fits_a_narrow_pane(self):
        cjk = "測" * 30  # 30 characters, 60 display cells
        self.make_persona(cjk)
        with mock.patch("os.get_terminal_size", return_value=os.terminal_size((20, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("right", "f-1")
        for line in out.splitlines():
            self.assertLessEqual(tui.width(line), 20)

    def test_a_failing_persona_chips_call_yields_a_line_rather_than_an_exception(self):
        """`_right` carries no guard of its own around the call
        (`_persona_chip_cells` already swallows its own failures) — this pins
        `render`'s own outer `try/except` as the thing that actually catches whatever
        gets past that, the same generic promise
        `Render.test_a_failing_renderer_yields_a_line...` pins for an arbitrary slot,
        exercised here through a real dependency."""
        with mock.patch("charter.statusline._persona_chip_cells",
                        side_effect=RuntimeError("boom")):
            self.assertIn("charter", slots.render("right", "f-1"))


class TheSidebarHasHeadings(PersonaIso, unittest.TestCase):
    """#516's first ask: a bare column of names told a newcomer nothing about what it was.

    The headings are `statusline.py`'s own (`_HEAD_PAD`, which carries `_MARK_HEAD`), so
    the frame's chrome and the status line's persona column cannot drift apart — asserted
    against that constant rather than against the literal `▪ `, which would pass a change
    that moved only one of the two surfaces.
    """

    def _render(self, *, cols=22, rows=26, fid="f-1") -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render("right", fid)

    def test_the_persona_list_is_headed(self):
        self.make_persona("alice")
        self.assertEqual(_plain_lines(self._render())[0],
                         f"{statusline._HEAD_PAD}personas 1")

    def test_the_heading_counts_every_persona_not_only_the_rows_that_fit(self):
        """The number and the `…(+N more)` line beneath it come from the same data
        (`PersonaChip.hidden`), so a truncated column cannot end up headed with the
        count of what survived truncation — which is the number a reader would take
        for "how many personas does this plane have"."""
        for i in range(9):
            self.make_persona(f"p{i:02d}")
        lines = _plain_lines(self._render(rows=6))
        self.assertEqual(lines[0], f"{statusline._HEAD_PAD}personas 9")
        self.assertTrue(any("…(+" in ln for ln in lines),
                        "it hid personas without a heading that says so")

    def test_a_plane_with_no_personas_still_says_so_rather_than_heading_nothing(self):
        lines = _plain_lines(self._render())
        self.assertIn("no personas", lines[0])
        self.assertFalse(any("personas 0" in ln for ln in lines), lines)


class TheSidebarBadgesFormAColumn(PersonaIso, unittest.TestCase):
    """#516's comment: the badges start wherever a name ends, so the column is ragged
    and a longer name pushes its badge past every other.

    The property is the SCREEN COLUMN the badge starts in, measured with `tui.width` on
    the plain text — not the character offset, which is what `len()` would give and what
    a wide glyph makes wrong. A test asserting the rendered string would pass on a row
    that merely happened to line up.
    """

    def _rows(self, cells, width=22):
        return slots._persona_rows(cells, width)

    def _badge_col(self, line: str, badge: str) -> int:
        plain = tui.strip_ansi(line)
        return tui.width(plain[:plain.index(badge)])

    def test_two_names_of_different_lengths_put_their_badges_in_one_column(self):
        cells = [statusline.PersonaChip("steward", "▸ steward", " A"),
                 statusline.PersonaChip("statusline", "▫ statusline", " B")]
        a, b = self._rows(cells)
        self.assertEqual(self._badge_col(a, "A"), self._badge_col(b, "B"))

    def test_a_row_with_no_badge_at_all_leaves_the_column_where_it_was(self):
        """The vault dot is present on some rows and absent on others, and so is every
        badge — a column sized per row would move under the rows that have one."""
        cells = [statusline.PersonaChip("steward", "▸ steward", " A"),
                 statusline.PersonaChip("quiet", "▫ quiet", ""),
                 statusline.PersonaChip("forge", "▫ forge ◦", " B")]
        a, _quiet, b = self._rows(cells)
        self.assertEqual(self._badge_col(a, "A"), self._badge_col(b, "B"))

    def test_the_vault_dot_stays_on_the_names_side_of_the_column(self):
        """`_vault_dot` speaks only when a vault cannot be used, so it is absent on
        almost every row: in the badge column its width would be paid by every persona
        for a fact about one of them."""
        cells = [statusline.PersonaChip("forge", "▫ forge ◦", " A"),
                 statusline.PersonaChip("reddit", "▫ reddit", " B")]
        a, b = self._rows(cells)
        self.assertEqual(self._badge_col(a, "A"), self._badge_col(b, "B"))
        self.assertIn("◦", tui.strip_ansi(a).split("A")[0])

    def test_a_wide_glyph_in_a_badge_is_measured_in_cells_not_characters(self):
        """`⚡` is East-Asian Wide — two cells, one character — so `len()` under-counts
        the widest badge and the column comes out a cell too narrow. The tell is not
        misalignment (every row is padded to the same wrong number, so they still line
        up) but SILENT LOSS: the badge that is actually the widest gets truncated inside
        its own cell, and `⚡2 4m` becomes `⚡2 4…`. That is the drift
        `_persona_chip_cells`' own comment says has broken this layout twice."""
        cells = [statusline.PersonaChip("a", "▫ a", " ⚡2 4m"),
                 statusline.PersonaChip("b", "▫ b", " ✎7")]
        a, b = self._rows(cells)
        self.assertEqual(self._badge_col(a, "⚡"), self._badge_col(b, "✎"))
        self.assertIn("⚡2 4m", tui.strip_ansi(a),
                      "the widest badge was cut by a column sized in characters")

    def test_a_name_too_long_for_its_cell_loses_its_own_tail_and_moves_nothing(self):
        """The whole defect, stated as a property: a longer name must cost ITS OWN row
        columns, never the column every other row's badge sits in."""
        cells = [statusline.PersonaChip("short", "▫ short", " A"),
                 statusline.PersonaChip("l" * 60, "▫ " + "l" * 60, " B")]
        a, b = self._rows(cells)
        self.assertEqual(self._badge_col(a, "A"), self._badge_col(b, "B"))
        self.assertLessEqual(tui.width(b), 22)

    def test_the_more_row_spans_the_pane_rather_than_being_padded_into_a_name_cell(self):
        """It names no persona and carries no badge — it is a sentence about the list,
        so lining it up with a column it has no entry in would only indent it."""
        cells = [statusline.PersonaChip("a", "▫ a", " ✎40"),
                 statusline.PersonaChip(None, "  …(+7 more)", "", 7)]
        _, note = self._rows(cells)
        self.assertEqual(tui.strip_ansi(note), "  …(+7 more)")

    def test_a_badge_column_never_squeezes_the_names_below_the_floor(self):
        """One persona holding three dispatches (`⚡3 2h?`) must not take twelve columns
        off every NAME in a 22-column sidebar — past `_NAME_MIN_W` the badge column is
        what gives way, not the names.

        The fixture is a LITERAL ten-character name rather than one built from
        `_NAME_MIN_W`, and that is deliberate: a fixture derived from the constant under
        test moves with it, so setting the floor to zero would shorten the name to
        nothing and the test would pass having asserted that the empty string survives.
        A concrete name that a real 22-column sidebar has to hold is what actually
        exercises the floor."""
        cells = [statusline.PersonaChip("reddit-ops", "▫ reddit-ops", " ✎47 ⚡3 2h?"),
                 statusline.PersonaChip("reddit", "▫ reddit", " ✎7")]
        rows = self._rows(cells)
        self.assertIn("reddit-ops", tui.strip_ansi(rows[0]),
                      "a wide badge took the name's own columns")
        self.assertIn("reddit", tui.strip_ansi(rows[1]))


class TheSidebarListsTheWorkspacesTodos(PersonaIso, unittest.TestCase):
    """#516's second ask. `_bottom` renders a COUNT; the items were visible nowhere in
    the frame.

    Every test renders through the real `slots.render("right", …)` rather than calling
    `_todo_rows`, for the reason `BottomTable` gives for the same choice: a helper
    returning perfect rows that `_right` never asks for satisfies a unit test of the
    helper and none of the promise.
    """

    def _render(self, *, cols=22, rows=26, fid="f-1") -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render("right", fid)

    def _seed_todos(self, titles, *, total=None, fid="f-1"):
        _seed(fid, todos=[{"title": t} for t in titles],
              todo_count=len(titles) if total is None else total)

    def test_a_todo_reaches_the_pane(self):
        self._seed_todos(["ship the sidebar"])
        self.assertIn("- ship the sidebar", _plain_lines(self._render(cols=40)))

    def test_the_todos_sit_beneath_the_personas(self):
        """The order is the ask, and a membership check would pass with them on top."""
        self.make_persona("alice")
        self._seed_todos(["ship the sidebar"])
        lines = _plain_lines(self._render(cols=40))
        self.assertLess([i for i, ln in enumerate(lines) if "alice" in ln][0],
                        [i for i, ln in enumerate(lines) if "ship the sidebar" in ln][0])

    def test_they_are_headed_with_the_open_count(self):
        self._seed_todos(["one", "two"])
        self.assertIn(f"{statusline._HEAD_PAD}todos 2", _plain_lines(self._render(cols=40)))

    def test_a_workspace_with_nothing_open_draws_no_section_at_all(self):
        """Not `todos 0`. A heading over an empty space is furniture within a day, and
        then a real todo appearing under it draws no more attention than the zero did.
        `_bottom` keeps its unconditional count; this is the column, not the strip."""
        self.make_persona("alice")
        self._seed_todos([])
        self.assertNotIn("todos", "\n".join(_plain_lines(self._render(cols=40))))

    def test_more_todos_than_fit_are_counted_rather_than_silently_dropped(self):
        self._seed_todos([f"todo number {i}" for i in range(30)], total=30)
        lines = _plain_lines(self._render(cols=40))
        shown = sum(1 for ln in lines if ln.startswith("- todo number"))
        self.assertGreater(shown, 0, lines)
        self.assertIn(f"  …(+{30 - shown} more)", lines)

    def test_the_hidden_count_is_the_true_total_not_the_cached_slice(self):
        """`gather._MAX_TODOS` bounds what the cache holds; `todo_count` is unclipped.
        Deriving the total from the list's length would tell an operator with four
        hundred open todos that they have twenty."""
        self._seed_todos([f"todo {i}" for i in range(20)], total=400)
        lines = _plain_lines(self._render(cols=40))
        shown = sum(1 for ln in lines if ln.startswith("- todo "))
        self.assertIn(f"{statusline._HEAD_PAD}todos 400", lines)
        self.assertIn(f"  …(+{400 - shown} more)", lines)

    def test_a_cache_written_before_the_count_existed_still_lists_what_it_has(self):
        """`_shaped_like_a_scan` is deliberately loose so a cache file surviving an
        upgrade still renders. A `todos` list with no `todo_count` beside it is exactly
        that file, and it must not report a negative or zero total."""
        _seed("f-1", todos=[{"title": "an older cache"}])
        lines = _plain_lines(self._render(cols=40))
        self.assertIn(f"{statusline._HEAD_PAD}todos 1", lines)
        self.assertIn("- an older cache", lines)
        self.assertFalse(any("…(+" in ln for ln in lines), lines)

    def test_a_short_pane_keeps_the_personas_and_gives_up_the_todos(self):
        """`right` is the persona column everywhere else charter names it, so a pane too
        short for both loses the section that is duplicated elsewhere (`charter ws todo`,
        and `bottom`'s own count) rather than the one that is not."""
        for i in range(6):
            self.make_persona(f"p{i}")
        self._seed_todos(["a todo nobody will see"])
        out = "\n".join(_plain_lines(self._render(rows=7, cols=40)))
        self.assertIn("p0", out)
        self.assertNotIn("a todo nobody will see", out)

    def test_a_pane_with_room_for_one_row_draws_no_section_at_all(self):
        """A heading with nothing under it claims this workspace has no todos, which is
        the false-clean reading the module refuses everywhere else. Two rows is the
        floor, and two rows is spent on the count and how much is hidden — the honest
        half of the pair, exactly as `_table_lines` spends a one-row budget."""
        for i in range(4):
            self.make_persona(f"p{i}")
        self._seed_todos(["one", "two"])
        # 4 persona rows + the heading is 5, and the blank separator takes a sixth.
        one_row = "\n".join(_plain_lines(self._render(rows=7, cols=40)))
        self.assertNotIn("todos", one_row)
        two_rows = _plain_lines(self._render(rows=8, cols=40))
        self.assertIn(f"{statusline._HEAD_PAD}todos 2", two_rows)
        self.assertIn("  …(+2 more)", two_rows)

    def test_a_todo_title_is_contained_before_it_reaches_a_row(self):
        """A todo is a COMMITTED value — someone else's machine wrote it into this
        plane's repo. A newline in one writes a second line that looks exactly as much
        like charter's own output as the first (#472's class), and the bound has to come
        before the width arithmetic, not after."""
        self._seed_todos(["first line\nSECOND ROW FORGED"])
        lines = _plain_lines(self._render(cols=60))
        # ONE row, with the newline shown as its own escape — not two rows, the second
        # of which looks exactly as much like charter's own output as the first.
        self.assertIn("- first line\\x0aSECOND ROW FORGED", lines)
        self.assertEqual(sum(1 for ln in lines if "SECOND ROW FORGED" in ln), 1, lines)

    def test_an_escape_in_a_todo_title_never_reaches_the_pane(self):
        self._seed_todos(["\x1b[2Jcleared your screen"])
        self.assertNotIn("\x1b[2J", self._render(cols=60))

    def test_a_cjk_todo_title_does_not_push_the_row_past_the_pane(self):
        self._seed_todos(["測" * 40])
        for line in self._render(cols=22).split("\n"):
            self.assertLessEqual(tui.width(line), 22)

    def test_the_rows_come_from_the_cache_and_never_from_the_todo_directory(self):
        """The idle-cost rule, one slot over from `bottom`'s table: `todos.open_todos`
        opens and parses one file per todo, and a panel repaints on every version bump.
        Pinned by making the live reader raise — a renderer that reaches it is red,
        rather than merely slow in a way no test can see."""
        self._seed_todos(["from the cache"])
        with mock.patch("charter.todos.open_todos",
                        side_effect=AssertionError("read the workspace, and must not")):
            lines = _plain_lines(self._render(cols=40))
        self.assertIn("- from the cache", lines)

    def test_a_frames_first_paint_lists_the_todos_rather_than_an_empty_column(self):
        """**The cache makes every repaint after the first one free; it is not what
        makes the first one right.**

        A launch DISCARDS the gather cache on purpose (`gather.discard` — a recycled pid
        must not adopt a dead frame's repos), so the very first paint of a new frame
        reaches this section with no cache file at all. `gather.read` falls through to a
        live `scan()` exactly there, which is the path that `discard`'s own docstring
        says deleting the file restores — so the first frame an operator sees carries
        their todos.

        Written because the round-1 news entry claimed the opposite, and the claim was
        never executed. It is also the guard against the cheap-looking simplification of
        `read` — answering `_empty()` on a cold cache rather than scanning — which would
        make a new frame's first impression a confident "this workspace has no todos".
        The seeded sibling above pins the other half: with a cache present, the todo
        directory is never touched at all.

        WHICH workspace that live gather is for is a separate question and a real one —
        `gather.scan` resolves it from the panel process, which #512 showed reaches none
        of the rungs that speak for the frame. Settled since, as **#526**: `todo_section`
        hands `gather.read` the frame's own workspace (`_frame_workspace`), so the scan
        this case forces is a scan for the workspace the frame was launched for.
        `tests/test_a_frame_answers_for_the_frames_workspace.py` is where that half is
        pinned, on a plane whose own rungs answer something else — which is the fixture
        this one deliberately does not build, because what it is about is the SCAN
        happening at all.
        """
        from charter import todos, workspace
        todos.add(workspace.resolve(), "written before the frame ever launched")
        gather.discard("f-cold")            # what `cmd_launch` runs before it draws
        with mock.patch.object(gather, "scan", wraps=gather.scan) as scanned:
            lines = _plain_lines(self._render(cols=50, fid="f-cold"))
        self.assertIn(f"{statusline._HEAD_PAD}todos 1", lines)
        self.assertIn("- written before the frame ever launched", lines)
        # And the cache really was cold: the rows came from the live fallback, not from
        # a file some earlier assertion had quietly left behind.
        self.assertTrue(scanned.called)


class TheTopBarStopsRepeatingTheSidebarsRoster(PersonaIso, unittest.TestCase):
    """#530: *"we still have personas in top-bar, why to have it if we already have
    personas list in right sidebar?"*

    Since #516 `_right` draws every persona with a heading, a memory badge, a vault dot, a
    health mark and an in-flight badge, in an aligned column. `_top` drew the same names
    flat — strictly less about exactly the same thing — so with a sidebar on screen the
    roster on the identity row said nothing new.

    **The property is a CONDITION, not a deletion**, and these tests pin both directions,
    because either one alone is satisfied by a wrong constant: `layout.visible_slots` drops
    `right` first on ANY shortage, so on a narrow or short terminal the top bar is the
    plane's only roster, and outside a frame there is no sidebar at all. A test that only
    asserted the roster's absence would pass a `_top` that never draws one.

    **What must survive either way is `◆ <active>`** — the roster is a list the sidebar
    can hold better, but the active persona is identity, and "who am I being" is read on
    this row next to the workspace.

    The panes are recorded through `state.record_panes`, which is where a launch
    (`commands_frame._draw_panels`) and a live density change (`cmd_density`) both write
    the frame's real shape — not through a stub of `_sidebar_live`, so what is exercised
    here is the record an operator's frame actually leaves behind.
    """

    #: None of these carry the words the assertions look for, so a hit is a label
    #: charter drew and never the fixture's own name.
    OTHERS = ("forge", "release")

    def setUp(self):
        super().setUp()
        self.make_persona("steward")
        for n in self.OTHERS:
            self.make_persona(n)
        persona.set_active("steward")

    def _top(self, fid, *, cols=200) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, 3))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return tui.strip_ansi(slots.render("top", fid))

    def _with_sidebar(self, fid="f-wide") -> str:
        state.record_panes(fid, panels={"top": "%1", "right": "%3", "bottom": "%2"})
        return self._top(fid)

    def _without_sidebar(self, fid="f-narrow") -> str:
        state.record_panes(fid, panels={"top": "%1", "bottom": "%2"})
        return self._top(fid, cols=60)

    def test_the_roster_is_gone_when_the_sidebar_is_drawing_it(self):
        row = self._with_sidebar()
        self.assertNotIn("◇ personas", row, row)
        for n in self.OTHERS:
            self.assertNotIn(n, row, f"`{n}` is on the top bar and in the sidebar: {row!r}")

    def test_the_active_persona_stays_because_it_is_identity_and_not_a_roster(self):
        """The half that must never go. The sidebar marks the active persona with `▸`
        inside a column of names; this row answers "who am I being" beside the workspace,
        which is a different question and this row's own."""
        row = self._with_sidebar()
        self.assertIn("◆ steward", row, row)

    def test_the_roster_comes_back_when_the_sidebar_is_not_on_screen(self):
        """`visible_slots` drops `right` first on any shortage, so this is the terminal
        where the top bar is the plane's only roster. Without this the fix above would be
        a deletion."""
        row = self._without_sidebar()
        self.assertIn("◇ personas", row, row)
        for n in self.OTHERS:
            self.assertIn(n, row, row)

    def test_a_frame_with_no_recorded_panes_keeps_its_roster(self):
        """The migration case and the corrupt-file case, which `state.panes` answers the
        same way. Charter cannot tell, so it draws the roster: at worst that is the
        duplication this issue is about, where the other direction would take the plane's
        only roster off a screen with no sidebar to replace it."""
        self.assertEqual({}, state.panes("f-unrecorded"))
        self.assertIn("◇ personas", self._top("f-unrecorded"))

    def test_the_answer_is_read_live_rather_than_at_launch(self):
        """#387's density hotkey adds and drops `right` while the frame runs, so a value
        decided once is wrong the moment the operator presses a key. Same fid, same
        process, same renderer — only the record changes in between."""
        fid = "f-density"
        state.record_panes(fid, panels={"top": "%1", "bottom": "%2"})
        self.assertIn("◇ personas", self._top(fid))
        state.record_panes(fid, panels={"top": "%1", "right": "%3", "bottom": "%2"})
        self.assertNotIn("◇ personas", self._top(fid))
        state.record_panes(fid, panels={"top": "%1", "bottom": "%2"})
        self.assertIn("◇ personas", self._top(fid),
                      "the sidebar went away and the roster did not come back")

    def test_density_decides_the_version_and_the_sidebar_decides_the_roster(self):
        """`terse` is the VERSION's business (see `_top`'s docstring), and this must not
        become a second, silent rule about the persona half. Both terse rows below drop
        the version; which of them carries a roster is decided by the sidebar and by
        nothing else.

        Both are reachable. `charter frame-density minimal` expands to `["top", "bottom"]`
        — no sidebar, so the terse row is the plane's only roster — while an explicit
        `[frame] slots` naming `right` alongside `density = "minimal"` wins over the
        preset (`instance.frame_of`), which is the terse frame that does have one.
        """
        for fid, panels, roster in (("f-terse-bare", {"top": "%1", "bottom": "%2"}, True),
                                    ("f-terse-side", {"top": "%1", "right": "%3"}, False)):
            with self.subTest(fid=fid):
                state.record_density(fid, "minimal")
                state.record_panes(fid, panels=panels)
                self.assertEqual("terse", slots.verbosity(fid))
                row = self._top(fid)
                self.assertNotIn("charter 0.", row, "terse kept the version")
                self.assertIn("◆ steward", row, row)
                self.assertEqual(roster, "◇ personas" in row, row)

    def test_top_picks_the_parts_rather_than_reassembling_the_row(self):
        """The shape #516 gave the chips, applied to the row (#530). `_top` chooses which
        pieces it draws and never what they say — pinned by handing it values nothing in
        `slots.py` could have produced, and requiring the two it keeps to reach the pane
        byte-for-byte while the one it drops does not."""
        parts = statusline.PersonaLine("SENTINEL-HEAD-0xF00D", "SENTINEL-ROSTER-0xBEEF",
                                       "SENTINEL-TAIL-0xCAFE")
        with mock.patch("charter.statusline._persona_line_parts", return_value=parts):
            with_bar = self._with_sidebar()
            without = self._without_sidebar()
        self.assertIn("SENTINEL-HEAD-0xF00D", with_bar)
        self.assertIn("SENTINEL-TAIL-0xCAFE", with_bar)
        self.assertNotIn("SENTINEL-ROSTER-0xBEEF", with_bar)
        self.assertIn("SENTINEL-ROSTER-0xBEEF", without)

    def test_a_plane_with_no_personas_draws_no_persona_half_at_all(self):
        """`_persona_line_parts` answers `None` there, and the row is still a row —
        the same promise the flat `_persona_line` made by answering `None`."""
        for n in ("steward", *self.OTHERS):
            shutil.rmtree(config.PERSONAS_DIR / n)
        persona.clear_active()
        row = self._with_sidebar()
        self.assertIn("⬢", row, row)
        self.assertNotIn("◆", row, row)


class TheStatusLineOutsideAFrameKeepsItsRoster(PersonaIso, unittest.TestCase):
    """The other caller (#530). `statusline._persona_line` feeds every session that is
    NOT in a frame, where there is no sidebar and the roster is the only place personas
    appear — so the split that let `_top` drop half of the row must leave that surface
    saying exactly what it said before, in exactly the order it said it.
    """

    def setUp(self):
        super().setUp()
        for n in ("steward", "forge", "release"):
            self.make_persona(n)

    def test_the_flat_line_still_carries_the_whole_roster(self):
        persona.set_active("steward")
        line = tui.strip_ansi(statusline._persona_line() or "")
        self.assertIn("◆ steward", line)
        self.assertIn("◇ personas", line)
        self.assertIn("forge", line)
        self.assertIn("release", line)

    def test_the_flat_line_is_exactly_the_three_parts_in_order(self):
        """The invariant that keeps the two surfaces from drifting: whatever the parts
        say, joined head-roster-tail, IS the row the status line prints. Asserted for
        both branches, because they are two different compositions."""
        for active in ("steward", None):
            with self.subTest(active=active):
                if active:
                    persona.set_active(active)
                else:
                    persona.clear_active()
                parts = statusline._persona_line_parts()
                self.assertEqual(parts.head + parts.roster + parts.tail,
                                 statusline._persona_line())

    def test_no_active_persona_keeps_the_command_that_gets_you_one(self):
        """The `tail`, and why it is a field of its own rather than part of the roster:
        a sidebar full of persona names still never says how to adopt one, so the tip
        survives the roster being dropped while the names do not."""
        persona.clear_active()
        parts = statusline._persona_line_parts()
        self.assertIn("persona none", tui.strip_ansi(parts.head))
        self.assertIn("charter persona use", tui.strip_ansi(parts.tail))
        self.assertNotIn("charter persona use", tui.strip_ansi(parts.roster))
        kept = tui.strip_ansi(parts.rendered(roster=False))
        self.assertIn("charter persona use", kept)
        self.assertNotIn("forge", kept, kept)

    def test_the_parts_need_no_separator_between_them(self):
        """`PersonaChip`'s contract, kept here too: each part carries its own leading
        separator, so dropping one never leaves a dangling ` · ` at the seam."""
        persona.clear_active()
        kept = tui.strip_ansi(statusline._persona_line_parts().rendered(roster=False))
        self.assertNotIn(" ·  · ", kept, kept)
        self.assertFalse(kept.rstrip().endswith("·"), kept)


if __name__ == "__main__":
    unittest.main()
