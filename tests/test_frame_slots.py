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

import os
import sys
import unittest
from unittest import mock

from charter import config, instance, statusline, tui
from charter.frame import gather, slots, state

from tests._isolation import PersonaIso


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
        len() still wraps the pane and pushes the frame apart."""
        for slot in ("top", "bottom"):
            for line in slots.render(slot, "f-1").splitlines():
                with self.subTest(slot=slot):
                    self.assertLessEqual(tui.width(line), tui.term_width(default=80))

    def test_a_failing_renderer_yields_a_line_rather_than_an_exception(self):
        """A panel that raises leaves a hole in the frame; `statusline.render` makes the
        same promise for the same reason."""
        slots.SLOTS["boom"] = lambda fid: 1 / 0
        try:
            self.assertIn("charter", slots.render("boom", "f-1"))
        finally:
            del slots.SLOTS["boom"]

    def test_the_bottom_row_names_the_configured_hotkey_not_a_hardcoded_one(self):
        """`[frame] hotkey` is configurable and this row spelled `F2 menu` literally, so
        a plane on `hotkey = "F1"` had its own panel telling every operator the wrong
        key, on every repaint, forever.

        `F1` is chosen precisely because it is NOT the default: asserting against `F2`
        would pass against the hardcoded string this test exists to remove. The absence
        assertion is the one that fails on the mutation."""
        with mock.patch.dict(config.FRAME, {"hotkey": "F1"}):
            out = slots.render("bottom", "f-1")
        self.assertIn("F1 menu", out)
        self.assertNotIn("F2", out)

    def test_a_modifier_hotkey_reaches_the_panel_intact(self):
        """A second, differently-shaped value — `F1` alone could be satisfied by a
        one-character substitution. `M-m` shares no characters with `F2`."""
        with mock.patch.dict(config.FRAME, {"hotkey": "M-m"}):
            self.assertIn("M-m menu", slots.render("bottom", "f-1"))

    def test_a_frame_in_the_operators_own_tmux_advertises_no_hotkey(self):
        """Charter binds no key at all inside a tmux it did not start — a key table is
        server-wide in tmux with no per-window form, and taking one from every window
        the operator has open to reach a menu whose only entry is "Detach" (which their
        own prefix already does) is a worse trade than none. A panel still printing
        `F2 menu` there would be telling every operator about a key that does nothing,
        on every repaint, forever — the same defect
        `test_the_bottom_row_names_the_configured_hotkey_not_a_hardcoded_one` exists
        for, reached through the other server instead of the wrong config value."""
        state.record_server("f-in-tmux", "/private/tmp/tmux-502/default")
        with mock.patch.dict(config.FRAME, {"hotkey": "F1"}):
            out = slots.render("bottom", "f-in-tmux")
        self.assertNotIn("menu", out)
        self.assertIn("todo", out, "the rest of the row is untouched")

    def test_a_frame_on_charters_own_server_still_advertises_it(self):
        """The other side of the same switch — a `_bottom` that simply stopped printing
        a hotkey would pass the test above on its own."""
        state.record_server("f-own", "charter")
        with mock.patch.dict(config.FRAME, {"hotkey": "F1"}):
            self.assertIn("F1 menu", slots.render("bottom", "f-own"))

    def test_an_unknown_slot_is_named_rather_than_drawn_blank(self):
        """`panel.run` (Task 7) refuses an unknown slot before ever spawning a pane for
        it — but `render` is the one place that can explain *why*, so it must not answer
        an unknown name with silence either."""
        out = slots.render("sideways", "f-1")
        self.assertIn("sideways", out)


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

    def test_width_falls_back_to_env_first_term_width_when_no_tty_is_available(self):
        """The one case `tui.term_width()` is allowed to answer: no tty behind the fd at
        all (stdout piped to a file, say), not merely a pane that disagrees with
        `$COLUMNS`."""
        with mock.patch.dict(os.environ, {"COLUMNS": "55"}), \
             mock.patch("os.get_terminal_size", side_effect=OSError("not a tty")):
            self.assertEqual(slots._width(), 55)


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
                    self.assertEqual("F2 menu" in out, want_hotkey)

    def test_a_failing_session_news_call_yields_a_line_rather_than_an_exception(self):
        with mock.patch("charter.statusline._session_news",
                        side_effect=RuntimeError("boom")):
            self.assertIn("charter", slots.render("bottom", "f-1"))

    def test_empty_fields_leave_no_stray_separator(self):
        """No alerts, no session news — `_fit_fields` must SKIP an empty field rather
        than keep it and let ` · `.join emit a blank slot between separators
        (`"5 todos ·  · F2 menu"`, say). Asserts the exact string rather than just
        `in`/`not in`, so a stray separator cannot hide inside a substring match."""
        with mock.patch("charter.statusline._alerts", return_value=[]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=5), \
             mock.patch.dict(config.FRAME, {"hotkey": "F2"}):
            out = tui.strip_ansi(slots.render("bottom", "f-1"))
        self.assertEqual(out, "5 todos · F2 menu")

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


class FitFields(unittest.TestCase):
    """`slots._fit_fields` in isolation, free of `_bottom`'s other, always-present
    fields (todo, hotkey) — so a test can construct the exact width pressure it wants
    without those competing for the same budget."""

    def test_the_first_field_is_kept_even_when_it_alone_exceeds_the_width(self):
        self.assertEqual(slots._fit_fields([("a", "AAAAAAAAAA")], 3), {"a"})

    def test_a_later_field_is_dropped_whole_once_the_budget_runs_out(self):
        self.assertEqual(
            slots._fit_fields([("a", "AAA"), ("b", "BBBBBBBBBB")], 6), {"a"})

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


class BottomTable(PersonaIso, unittest.TestCase):
    """#488: the repo table `bottom` draws under its attention row.

    The rows are `statusline.py`'s OWN wide table — same four columns, same declared
    widths, same markers and CI glyphs — composed straight from `gather`'s cache: never a
    `git` call, never `glstate`, and never a repo directory (see `_table_row`'s docstring
    for the one column that costs a filesystem walk per row and is therefore absent).

    Every test here renders through the real `slots.render("bottom", …)` rather than
    calling `_table_lines` directly, because what #488 actually promises is that a
    PANEL shows this — a helper returning perfect rows that `_bottom` never asks for
    would satisfy a unit test of the helper and none of the promise.
    """

    def _render(self, fid="f-1", *, cols=200, rows=24) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render("bottom", fid)

    def test_lists_a_repo_from_the_cache(self):
        _seed("f-1", repos=[_row("demo")])
        self.assertIn("demo", self._render())

    def test_the_attention_row_is_still_the_first_line(self):
        """#488's non-negotiable: the table JOINS the alert, the news, the todo count and
        the plane-root warning — it does not evict them. Asserted as line 0 specifically,
        because a table drawn above the row it is meant to sit under would still contain
        both strings and satisfy a membership check."""
        _seed("f-1", repos=[_row("demo")])
        out = self._render()
        self.assertIn("todo", tui.strip_ansi(out.split("\n")[0]))
        self.assertIn("demo", out)

    def test_a_plane_with_no_repos_is_the_one_row_strip_it_always_was(self):
        """The floor `layout.bottom_rows` keeps, seen from the renderer's side. The
        reported "always empty sidebar" of #488 was this case being told the truth —
        a workspace with 0 clones — so it must stay honest rather than grow furniture."""
        _seed("f-1")
        self.assertEqual(len(self._render().split("\n")), 1)

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
        self.assertIn("more)", out)
        self.assertNotIn("all clean", out)

    def test_the_table_is_bounded_by_the_panes_own_measured_height(self):
        """The renderer must spend the pane it HAS, not the one the launcher intended:
        a resize changes the pane under a running panel and nothing bumps the frame's
        version for it. Asserted at two heights against the same cache, so a renderer
        ignoring the measurement and emitting everything is red at the short one."""
        _seed("f-1", repos=[_row(f"repo{i}") for i in range(10)])
        self.assertEqual(len(self._render(rows=6).split("\n")), 6)
        self.assertEqual(len(self._render(rows=20).split("\n")), 1 + 10)

    def test_the_height_the_launcher_asks_for_is_the_height_the_renderer_fills(self):
        """The seam #488 turns on. `slots.bottom_rows_wanted` is what tells
        `layout.slot_sizes` how tall to split the pane; `_bottom` is what fills it. If
        the two disagreed, every frame would come up either padded with blank rows the
        harness could have had, or with a table cut off and nothing saying so — and
        neither is visible from either side alone. Rendered into a pane of EXACTLY the
        height the sizer asked for, and counted."""
        for n in (0, 1, 4, 9):
            with self.subTest(repos=n):
                fid = f"wanted-{n}"
                _seed(fid, repos=[_row(f"repo{i}") for i in range(n)])
                want = slots.bottom_rows_wanted(fid)
                out = self._render(fid, rows=want)
                self.assertEqual(len(out.split("\n")), want, out)

    def test_a_pane_too_narrow_for_the_table_draws_no_table_rather_than_a_cut_one(self):
        """Every column after the branch sits at a fixed offset past
        `_NAME_W + _BRANCH_W`, so a narrow pane loses the CI glyph and the open change
        off the right-hand end — and a dirty, CI-failing repo then renders as a clean
        `charter  main`. Refusing to draw says "no room to say"; a trimmed row says
        "nothing to say", which is the false-clean failure the plan's Global Constraints
        name. The attention row is unaffected — it budgets its own fields."""
        _seed("f-1", repos=[_row("demo", dirty=True, ci="failed")])
        out = self._render(cols=statusline._LEFT_W - 1)
        self.assertEqual(len(out.split("\n")), 1, out)
        self.assertIn("todo", tui.strip_ansi(out))
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

    def test_a_failing_gather_read_yields_a_line_rather_than_an_exception(self):
        """A panel that raises leaves a hole in the frame — `slots.render`'s own promise,
        pinned against a renderer that reaches into a real dependency."""
        with mock.patch.object(gather, "read", side_effect=RuntimeError("boom")):
            self.assertIn("charter", slots.render("bottom", "f-1"))


class BottomRowsWanted(PersonaIso, unittest.TestCase):
    """`slots.bottom_rows_wanted` — the number the LAUNCHER sizes the pane from."""

    def test_a_plane_with_no_repos_wants_exactly_the_attention_row(self):
        _seed("f-1")
        self.assertEqual(slots.bottom_rows_wanted("f-1"), 1)

    def test_it_grows_with_the_repos_and_the_pieces_alike(self):
        _seed("f-1", repos=[_row("a"), _row("b")],
              worktrees=[_row("p", repo="a")])
        self.assertEqual(slots.bottom_rows_wanted("f-1"), 1 + 3)

    def test_it_is_capped_at_the_wide_tables_own_row_budget(self):
        """A workspace with forty clones must ask for a fifteen-row strip, not a
        forty-one-row one — `_MAX_REPO_LINES` is the same total-row budget the wide table
        keeps, reused rather than invented fresh."""
        _seed("f-1", repos=[_row(f"r{i}") for i in range(40)])
        self.assertEqual(slots.bottom_rows_wanted("f-1"),
                         1 + statusline._MAX_REPO_LINES)

    def test_it_never_runs_a_git_sweep(self):
        """It is called on a launch the operator is waiting on, and again on every step
        of a terminal drag. `gather.row_count` answers from the cache when there is one
        and from a directory listing when there is not; a `scan()` on either path would
        put a `git status` per repo inside a `window-resized` hook."""
        _seed("f-1", repos=[_row("a")])
        with mock.patch("charter.frame.gather.scan",
                        side_effect=AssertionError("row_count ran a scan")):
            self.assertEqual(slots.bottom_rows_wanted("f-1"), 2)


class RightRenderer(PersonaIso, unittest.TestCase):
    """`right`: `statusline._persona_chips` called, not reassembled — each chip
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
        `_persona_chips` a value nothing in `_right` could have produced on its
        own, and requiring it survive to the pane byte-for-byte."""
        with mock.patch("charter.statusline._persona_chips",
                        return_value=["SENTINEL-CHIP-0xF00D"]):
            out = slots.render("right", "f-1")
        self.assertIn("SENTINEL-CHIP-0xF00D", out)

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
        """`_right` carries no guard of its own around the call (`_persona_chips`
        already swallows its own failures) — this pins `render`'s own outer
        `try/except` as the thing that actually catches whatever gets past that,
        the same generic promise `Render.test_a_failing_renderer_yields_a_line...`
        pins for an arbitrary slot, exercised here through a real dependency."""
        with mock.patch("charter.statusline._persona_chips",
                        side_effect=RuntimeError("boom")):
            self.assertIn("charter", slots.render("right", "f-1"))


if __name__ == "__main__":
    unittest.main()
