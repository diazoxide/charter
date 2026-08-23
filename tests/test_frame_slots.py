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

from charter import config, statusline, tui
from charter.frame import gather, slots

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

    def test_all_four_slots_now_have_a_renderer(self):
        """Task 3 landed `left`/`right` beside `top`/`bottom`: every slot
        `instance.FRAME_SLOTS` accepts now has a renderer, so a fully-configured
        frame names nothing missing."""
        self.assertEqual(slots.unimplemented(["top", "left", "bottom", "right"]), [])

    def test_an_all_implemented_configuration_names_nothing(self):
        self.assertEqual(slots.unimplemented(["top", "bottom"]), [])

    def test_the_answer_comes_from_the_registry_not_a_hardcoded_pair(self):
        """`left`/`right` having renderers today is not the rule this function follows
        — the registry is. Proved here from the other direction now that both are
        implemented: temporarily REMOVE one from the registry and the answer must
        follow, exactly as it would the day a real slot's renderer regresses."""
        with mock.patch.dict(slots.SLOTS):
            del slots.SLOTS["right"]
            self.assertEqual(slots.unimplemented(["top", "left", "right"]), ["right"])


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
    add nothing but a docstring gone stale."""

    def test_the_context_gauge_is_never_called_by_top(self):
        with mock.patch("charter.statusline._context_gauge") as gauge:
            slots.render("top", "f-1")
        gauge.assert_not_called()

    def test_the_session_strip_is_never_called_by_top(self):
        with mock.patch("charter.statusline._session_strip") as strip:
            slots.render("top", "f-1")
        strip.assert_not_called()

    def test_context_gauge_confirmed_empty_with_no_payload_the_premise_top_relies_on(self):
        """The load-bearing fact behind leaving the gauge out: with no live per-turn
        payload — exactly what a panel process always has — `_context_gauge` produces
        nothing, not a misleading zero. `tests/test_statusline_gauge.py` already pins
        this generically (`test_silent_before_first_api_call`); repeated here because
        it is the premise `_top`'s design decision rests on."""
        self.assertEqual(statusline._context_gauge(None), [])
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
        and requiring it actually reach the call."""
        with mock.patch("charter.session.current", return_value="SID-SENTINEL-0xF00D"), \
             mock.patch("charter.statusline._session_news", return_value=[]) as news:
            slots.render("bottom", "f-1")
        news.assert_called_once_with("SID-SENTINEL-0xF00D")

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


class LeftRenderer(PersonaIso, unittest.TestCase):
    """`left`: repo rows composed narrow straight from `gather`'s cache — never a
    `git` call, never `glstate`, never `_repo_rows`' `tui.Node`s (built for a wide
    boxed frame; `_NAME_W`=32 alone exceeds this whole pane)."""

    def test_lists_a_repo_from_the_cache(self):
        _seed("f-1", repos=[_row("demo")])
        self.assertIn("demo", slots.render("left", "f-1"))

    def test_degrades_to_a_readable_line_with_an_empty_cache(self):
        _seed("f-1")
        out = slots.render("left", "f-1")
        self.assertTrue(out.strip())
        self.assertIn("no repos", tui.strip_ansi(out))

    def test_a_dirty_repo_shows_the_dirty_marker(self):
        _seed("f-1", repos=[_row("demo", dirty=True)])
        self.assertIn("*", tui.strip_ansi(slots.render("left", "f-1")))

    def test_a_clean_repo_shows_no_dirty_marker(self):
        _seed("f-1", repos=[_row("demo", dirty=False)])
        self.assertNotIn("*", tui.strip_ansi(slots.render("left", "f-1")))

    def test_an_open_change_shows_its_sigil_and_number(self):
        _seed("f-1", repos=[_row("demo", change=42, sigil="!")])
        self.assertIn("!42", tui.strip_ansi(slots.render("left", "f-1")))

    def test_a_failing_ci_status_shows_its_glyph(self):
        _seed("f-1", repos=[_row("demo", ci="failed")])
        self.assertIn("✗", tui.strip_ansi(slots.render("left", "f-1")))

    def test_a_piece_from_the_worktrees_cache_field_is_shown(self):
        _seed("f-1", repos=[_row("demo")],
             worktrees=[_row("piece-one", repo="demo")])
        self.assertIn("piece-one", tui.strip_ansi(slots.render("left", "f-1")))

    def test_a_multi_repo_workspaces_piece_count_shows_as_a_badge(self):
        """Fix round 1, finding 2: `worktree_count` did not exist in the cache at
        all for a multi-repo workspace before this round — `data["worktrees"]`
        is `[]` here (as it always is with two repos; `gather._detail_worktrees`'
        own single-repo rule), so the badge is the ONLY way either repo's pieces
        are visible at all."""
        _seed("f-1", repos=[_row("demo", worktree_count=3),
                            _row("second", worktree_count=0)],
             worktrees=[])
        out = tui.strip_ansi(slots.render("left", "f-1"))
        self.assertIn("⑂3", out)

    def test_a_repo_whose_pieces_are_all_shown_as_rows_carries_no_badge(self):
        """The single-repo case: every piece already has its own row
        (`data["worktrees"]`), so the badge — "there is more you cannot see" —
        would be actively misleading if it appeared anyway."""
        _seed("f-1", repos=[_row("demo", worktree_count=1)],
             worktrees=[_row("piece-one", repo="demo")])
        out = tui.strip_ansi(slots.render("left", "f-1"))
        self.assertNotIn("⑂", out)

    def test_picks_the_dirty_repo_over_clean_ones_when_over_budget(self):
        """`_pick_rows` is CALLED here, not reinvented — the same ranking
        `statusline.py`'s own regression (an unranked slice of 18 clones showed
        thirteen clean repos and hid the one dirty one) was filed against. A plain
        `dirs[:budget]` slice would keep `clean-0..clean-N` (they sort first) and
        drop `zzz-dirty` off the end."""
        clean = [_row(f"clean-{i}") for i in range(statusline._MAX_REPO_LINES)]
        dirty = _row("zzz-dirty-one-past-the-cap", dirty=True)
        _seed("f-1", repos=clean + [dirty])
        self.assertIn("zzz-dirty-one-past-the-cap",
                      tui.strip_ansi(slots.render("left", "f-1")))

    def test_the_overflow_note_matches_the_wide_tables_own_wording(self):
        """Fix round 1, finding 3: this line used to say `", clean"` where
        `_repo_rows`' own overflow line (`statusline.py`) says `", all clean"` —
        the same claim, worded two different ways on the two surfaces."""
        clean = [_row(f"clean-{i}") for i in range(statusline._MAX_REPO_LINES + 1)]
        _seed("f-1", repos=clean)
        out = tui.strip_ansi(slots.render("left", "f-1"))
        self.assertIn(", all clean)", out)
        self.assertNotIn("more, clean)", out)

    def test_never_exceeds_the_pane_width(self):
        _seed("f-1", repos=[_row("a-repo-with-quite-a-long-descriptive-name",
                                change=999999, ci="failed", ahead=12, behind=34)])
        with mock.patch("os.get_terminal_size", return_value=os.terminal_size((22, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("left", "f-1")
        for line in out.splitlines():
            with self.subTest(line=line):
                self.assertLessEqual(tui.width(line), 22)

    def test_never_exceeds_the_pane_width_even_when_narrower_than_the_markers(self):
        """`_row`'s own per-field budgeting can still ask for more than a truly
        tiny pane has (each field's floor is `max(1, ...)`, and those floors can
        sum past `width` once the pane is narrower than the markers themselves)
        — `_row`'s trailing `tui.truncate(line, width)` is the backstop for
        exactly that. Confirmed load-bearing by mutation: dropping it lets a
        5-repo-state line overflow a 3-column pane to 8 display cells."""
        _seed("f-1", repos=[_row("abcdef", dirty=True, ahead=3, behind=2,
                                ci="failed", change=5, sigil="!")])
        with mock.patch("os.get_terminal_size", return_value=os.terminal_size((3, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("left", "f-1")
        for line in out.splitlines():
            with self.subTest(line=line):
                self.assertLessEqual(tui.width(line), 3)

    def test_a_realistic_long_branch_does_not_crowd_out_the_dirty_marker_or_ci_glyph(self):
        """Fix round 1, finding 1: a single `tui.truncate` over the whole assembled
        line cut from the right, and this project's own branches
        (`worktree-recall-since`, `browser-session-scope`, `global-shim-refresh`:
        21-28 characters, the norm here) are long enough that `name + " " +
        branch` alone fills a 22-column pane before the dirty marker, the CI
        glyph or an open change is ever reached — a FALSE CLEAN reading on a
        dirty, CI-failing, unpushed repo. `test_a_dirty_repo_shows_the_dirty_marker`
        and `test_a_failing_ci_status_shows_its_glyph` above use `branch="main"`
        (no truncation pressure) and would not have caught this; this fixture
        matches the reviewer's own repro exactly."""
        _seed("f-1", repos=[_row("charter", branch="worktree-recall-since",
                                dirty=True, ahead=1, ci="failed")])
        with mock.patch("os.get_terminal_size", return_value=os.terminal_size((22, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = tui.strip_ansi(slots.render("left", "f-1"))
        self.assertIn("*", out, "the dirty marker must survive truncation")
        self.assertIn("✗", out, "the CI glyph must survive truncation")

    def test_a_long_branch_with_a_change_still_surfaces_marker_ci_and_change(self):
        """The three-field version of the test above, at the ACTUAL production
        pane width (`layout.SLOT_SIZE["left"] == 22`) — pins that reserving room
        for CI does not itself starve a trailing open change (`_row`'s priority
        order includes it last), the case a narrower, artificial width would
        have to invent rather than measure."""
        _seed("f-1", repos=[_row("charter", branch="worktree-recall-since",
                                dirty=True, ahead=1, ci="failed",
                                change=12, sigil="!")])
        with mock.patch("os.get_terminal_size", return_value=os.terminal_size((22, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            line = slots.render("left", "f-1").splitlines()[0]
        out = tui.strip_ansi(line)
        self.assertLessEqual(tui.width(line), 22)
        self.assertIn("*", out, "the dirty marker must survive")
        self.assertIn("✗", out, "the CI glyph must survive")
        self.assertIn("!12", out, "the open change must survive")

    def test_a_cjk_heavy_repo_name_still_fits_a_narrow_pane(self):
        """Fix round 2, finding 1: pins the width invariant end-to-end through
        `_left`/`_repo_line` for a real CJK repo name — it does NOT by itself
        pin that `_row`'s name truncation is cell-aware rather than
        char-aware. A repo's `name_markup` carries an ANSI colour prefix
        (`_PALETTE`), and naive `name_markup[:name_w]` character-slicing
        spends several of the budgeted characters on the escape bytes rather
        than the CJK text — which happens to *under*-consume the intended
        cell budget for this fixture, so this test stays green under that
        mutation (verified: reverting `_row`'s `tui.truncate(name_markup,
        name_w)` to `name_markup[:name_w]` does not red this test, with or
        without the trailing safety-net truncate). `_piece_line`'s CJK test
        below is the one that actually pins cell-awareness — a piece name
        carries no ANSI prefix to accidentally absorb the mistake."""
        cjk = "測" * 30  # 30 characters, 60 display cells
        _seed("f-1", repos=[_row(cjk)])
        with mock.patch("os.get_terminal_size", return_value=os.terminal_size((20, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("left", "f-1")
        for line in out.splitlines():
            self.assertLessEqual(tui.width(line), 20)

    def test_a_cjk_heavy_piece_name_does_not_crowd_out_its_branch_or_markers(self):
        """Fix round 2, finding 1: the genuinely fragile field. A piece's name
        (`_piece_line`, `p["name"]`) carries no ANSI prefix, so nothing
        absorbs a char-vs-cell counting mistake the way the repo-name test
        above happens to. With `tui.truncate` doing the real truncation, the
        branch and its markers survive alongside a correctly-shrunk CJK name
        (`'╰─ 測測測… main* ✗'`); under naive `name_markup[:name_w]` slicing
        the name alone consumes 2x its intended cell budget (each CJK
        character sliced in COUNT costs 2 cells), so the branch, the dirty
        marker and the CI glyph are all pushed out entirely by the trailing
        safety-net truncate — the SAME false-clean failure mode fix round 1
        closed for the branch field, reopened here through the name field."""
        cjk = "測" * 30  # 30 characters, 60 display cells
        _seed("f-1", repos=[_row("demo", worktree_count=1)],
             worktrees=[_row(cjk, repo="demo", branch="main",
                             dirty=True, ci="failed")])
        with mock.patch("os.get_terminal_size", return_value=os.terminal_size((20, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("left", "f-1")
        for line in out.splitlines():
            with self.subTest(line=line):
                self.assertLessEqual(tui.width(line), 20)
        piece_line = tui.strip_ansi(out.splitlines()[-1])
        self.assertIn("main", piece_line, "the branch must survive")
        self.assertIn("*", piece_line, "the dirty marker must survive")
        self.assertIn("✗", piece_line, "the CI glyph must survive")

    def test_a_starved_pane_drops_lowest_priority_fields_first(self):
        """Fix round 2, finding 2: reachable in production despite the
        22-column DEFAULT — `layout.py`'s own module docstring measures real
        tmux 3.7c redistributing every pane proportionally on a resize, `-l
        size` notwithstanding ("growing a 120x30 frame to 200x50 stretched two
        one-row panels to 8 and 7 rows" before a corrective `window-resized`
        hook snapped them back): an ORDINARY window resize, not a future
        configurability feature. `_width()` measures the real pane rather
        than trusting `layout.SLOT_SIZE` for exactly this reason.

        Pins `_row`'s priority order (CI drops before the dirty marker, which
        `_left`'s own overflow-quiet logic and `_tree_cells`' wide-table
        counterpart both treat as the higher-priority fact) rather than
        merely the total width bound `test_never_exceeds_the_pane_width_...`
        above already covers."""
        _seed("f-1", repos=[_row("ab", branch="main", dirty=True, ci="failed")])
        # (width, marker expected, CI glyph expected) — matches this fixture
        # measured directly: 'ab main* ✗' / 'ab m…* ✗' / 'ab m…*' / 'ab …'.
        cases = [(10, True, True), (8, True, True), (6, True, False), (4, False, False)]
        for width, want_marker, want_ci in cases:
            with mock.patch("os.get_terminal_size", return_value=os.terminal_size((width, 24))), \
                 mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
                line = tui.strip_ansi(slots.render("left", "f-1").splitlines()[0])
            with self.subTest(width=width):
                self.assertLessEqual(tui.width(line), width)
                self.assertEqual("*" in line, want_marker)
                self.assertEqual("✗" in line, want_ci)

    def test_a_failing_gather_read_yields_a_line_rather_than_an_exception(self):
        """A panel that raises leaves a hole in the frame — `slots.render`'s own
        promise, pinned here against a renderer that actually reaches into a real
        dependency (`gather.read`) rather than the generic lambda `Render`'s own
        `test_a_failing_renderer...` uses."""
        with mock.patch.object(gather, "read", side_effect=RuntimeError("boom")):
            self.assertIn("charter", slots.render("left", "f-1"))


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
