"""A tab strip launches one row deep and one key cycles it 1 → 2 → 3 (#880).

**The default changed and the way out of it is a keypress.** `slots.bar_rows_wanted`
composes at 1, 2 … `layout.BAR_MAX_ROWS` and keeps the tallest height it FILLS, and
`layout._grown` hands those rows out of the harness's spare budget — so a plane with many
names came up two rows deep whether or not its operator wanted the harness two rows
shorter. The measured objection to one row no longer stands on its own: `+N` is clickable
and opens the palette, which lists every name, so a collapsed strip is one press from the
complete list rather than a dead end.

**What the key changes is a CEILING**, which is the property that keeps the whole feature
honest. `bar_rows_wanted` still answers with the rows a strip actually fills, so a press on
a plane whose names already fit changes nothing on screen — and a press on one that
overflows buys exactly the rows there are names for. `TheKeyRaisesACeilingAndNotAHeight`
is that half.

**The chosen height does not survive a restart.** It is a file in the frame's own state
directory, which `state.reap` deletes whole when the frame ends and `state.clear_shape`
deletes when a new frame claims a recycled id — the same place `density` and `hidden`
live, so there is no new kind of state and nothing for `doctor` to explain. A plane that
always wants three rows says so once in `[[frame.component]] size` (#687), which
`layout._grown` still honours.

**Three alternatives were weighed and this is the one that survived.** A tmux drag-resize
is the gesture an operator would reach for and is recomputed away on the next layout pass,
because the layout owns bar heights (both bars are `Fixed(1)`). `charter.toml` alone
already works and is an edit-and-restart rather than a gesture. A key costs one more
server-wide `bind -n`, and `TheKeyIsCharactersOwnAndNotAComponentsToTake` is where that
cost is held down.
"""

from __future__ import annotations

import os
import subprocess
import sys
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import layout, overlay, slots, state, tmuxctl

from tests import _tmuxchain
from tests._isolation import PersonaIso

#: The fifteen workspaces this repository's own plane has — `test_a_tab_strip_grows_a_row
#: _when_its_tabs_overflow`'s list, repeated rather than imported for the reason that
#: module's own `_a_dead_pid` copy states.
NAMES = sorted([
    "authority-audit", "autonomy", "charter-update-skill", "default", "fleet",
    "harness-wrapper", "news-dispatch-guard", "opencode-integration", "plane-shape",
    "relations-and-delegations", "showcase", "statusline-improvements", "todos",
    "tracking-github-issues", "user-reporting",
])

SLOTS = ["top", "chats", "workspaces", "bottom", "repos", "right"]


def _a_dead_pid() -> int:
    """A real pid that has exited and been reaped."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


def _placed(**sizes):
    """The resolved arrangement that places both bars, built the way an operator's
    `charter.toml` reaches `config.FRAME` — a bar has to be PLACED for `layout` to have
    any geometry for it at all (#687).

    *sizes* pins a component to a committed height, which is the one route a bar's own
    `[[frame.component]] size` has and the thing #880 leaves alone."""
    tables = []
    for use in ("identity", "chats", "workspaces", "attention", "repos", "sidebar"):
        table = {"use": use}
        if use in sizes:
            table.update(edge="top", size=sizes[use])
        tables.append(table)
    return instance.frame_of({"frame": {"component": tables}})


class _Tmux:
    """A recording stand-in for `tmuxctl.run` — `tests/test_component_toggle_keys.py`'s
    fake, repeated for the reason that module's own copy states. It answers the two
    queries a re-layout reads a value out of and reports success for everything else."""

    def __init__(self, *, size="200:50", new_panes=("%7", "%8", "%9")):
        self.size = size
        self.new_panes = list(new_panes)
        self.calls: list[list[str]] = []

    def __call__(self, action, argv, *, env=None, timeout=None, report=True):
        return _tmuxchain.answer_run(self._one, action, argv, env=env, timeout=timeout,
                                     report=report)

    def _one(self, action, argv, *, env=None, timeout=None, report=True):
        self.calls.append(list(argv))
        out = ""
        if "display-message" in argv:
            out = self.size
        elif "split-window" in argv:
            out = self.new_panes.pop(0) if self.new_panes else "%9"
        return SimpleNamespace(returncode=0, stdout=out, stderr="")

    def resized(self, pane):
        """Every `-y` a `resize-pane` asserted for *pane*, in the order it asserted it."""
        return [c[c.index("-y") + 1] for c in self.calls
                if "resize-pane" in c and "-y" in c and c[c.index("-t") + 1] == pane]


class TheCycleIsOneToTwoToThreeAndBack(unittest.TestCase):
    """`layout.next_bar_rows` and `layout.bar_rows_cap` — the arithmetic, with no plane
    and no tmux under it, for `test_frame_bars`' reason: it has to be right at every value
    and a case that needed a frame would be measuring the fixture."""

    def test_a_frame_that_has_chosen_nothing_is_at_the_default(self):
        self.assertEqual(layout.BAR_ROWS_DEFAULT, 1)
        self.assertEqual(layout.bar_rows_cap(None), 1)

    def test_the_cycle_wraps_at_the_ceiling(self):
        """Every press has to do something — `builtin_actions._register_strip`'s rule for
        the tab walk one surface over. A key that silently did nothing at the top is one an
        operator presses twice and then stops trusting."""
        self.assertEqual([layout.next_bar_rows(n) for n in (None, 1, 2, 3)],
                         [2, 2, 3, 1])

    def test_the_ceiling_the_cycle_reaches_is_the_shipped_one(self):
        """Two constants and not three: the cycle's top IS `BAR_MAX_ROWS`, which is where
        the measurement behind it lives."""
        self.assertEqual(layout.BAR_MAX_ROWS, 3)
        self.assertEqual(max(layout.next_bar_rows(n) for n in range(0, 9)),
                         layout.BAR_MAX_ROWS)

    def test_a_value_outside_the_range_degrades_to_the_default(self):
        """`instance.density_level`'s discipline. The number is read back off a file, so
        `0`, `7` and a truncated write are "this frame has not chosen" and not "this frame
        chose something charter will round for it". Clamping a `7` to `3` would leave a
        frame silently at the ceiling because a byte went missing."""
        for bad in (0, -1, 4, 7, 10_000):
            with self.subTest(bad=bad):
                self.assertEqual(layout.bar_rows_cap(bad), layout.BAR_ROWS_DEFAULT)
                self.assertEqual(layout.next_bar_rows(bad),
                                 layout.BAR_ROWS_DEFAULT + 1)

    def test_the_reader_and_the_cycle_share_one_clamp(self):
        """Two clamps are two chances to disagree about whether `3` wraps. Asked as the
        property: whatever the cycle answers is a value the reader accepts unchanged."""
        for n in (None, *range(-2, 9)):
            with self.subTest(n=n):
                self.assertEqual(layout.bar_rows_cap(layout.next_bar_rows(n)),
                                 layout.next_bar_rows(n))


class TheHeightIsRememberedForThisFrameAndNoLonger(PersonaIso, unittest.TestCase):
    """`state.record_bar_rows` / `state.bar_rows` — where the choice lives and what
    happens to it."""

    FID = "api.1"

    def test_a_frame_nobody_has_pressed_it_in_has_no_file_and_no_answer(self):
        """``None`` is the ordinary case and not a failure: every frame comes up at the
        default because there is nothing on disk to say otherwise. That IS "the height
        does not survive a restart", with no new kind of state behind it."""
        self.assertIsNone(state.bar_rows(self.FID))

    def test_what_was_written_is_what_is_read_back(self):
        for n in (1, 2, 3):
            state.record_bar_rows(self.FID, n)
            self.assertEqual(state.bar_rows(self.FID), n)

    def test_a_new_frame_claiming_a_recycled_id_does_not_inherit_it(self):
        """`state.clear_shape`'s list. A brand-new frame inheriting three rows would come
        up with a three-row-shorter harness taken from somebody else's session, with
        nothing on screen to say why — and it would break the sentence #880 is written to
        keep."""
        state.record_bar_rows(self.FID, 3)
        state.clear_shape(self.FID)
        self.assertIsNone(state.bar_rows(self.FID))

    def test_an_unreadable_or_unparseable_file_is_the_default_and_not_a_raise(self):
        """This is read on a sizing path that runs inside the `frame-resize` child, so a
        half-written file must degrade rather than take the recompute down. The RANGE is
        not checked here — `layout.bar_rows_cap` is the one gate, at the point of use."""
        d = state.frame_dir(self.FID, create=True)
        (d / "bar_rows").write_text("not a number\n")
        self.assertIsNone(state.bar_rows(self.FID))
        (d / "bar_rows").write_text("7\n")
        self.assertEqual(state.bar_rows(self.FID), 7)
        self.assertEqual(layout.bar_rows_cap(state.bar_rows(self.FID)),
                         layout.BAR_ROWS_DEFAULT)

    def test_it_is_written_in_the_frames_own_state_directory(self):
        """`cmd_toggle`'s argument, one file over: `[[frame.component]] size` says what a
        strip STARTS at and this says what one running frame IS — so it goes where the
        machine-written per-frame files go, beside `density` and `hidden`, and `reap`
        deletes it with them.

        Asserted as WHERE rather than as "charter.toml is unchanged", which is the claim
        that has teeth: `tests/_planeguard.py` already refuses a write to the operator's
        own file, so a case about that one would pass whatever this function did."""
        state.record_bar_rows(self.FID, 2)
        d = state.frame_dir(self.FID)
        self.assertTrue((d / "bar_rows").is_file())
        self.assertEqual((d / "bar_rows").read_text(), "2\n")
        self.assertFalse((d / "bar_rows.tmp").exists(),
                         "the atomic write left its temp file behind")


class TheKeyRaisesACeilingAndNotAHeight(PersonaIso, unittest.TestCase):
    """The boundary, end to end with a real plane and no tmux —
    `commands_frame._slot_sizes` is where a frame's own state becomes a pane height."""

    def setUp(self):
        super().setUp()
        for name in NAMES:
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(config.FRAME, _placed()))

    def _rows(self, fid, cols):
        return commands_frame._slot_sizes(fid, SLOTS, window_rows=50, pane_cols=cols,
                                          order=SLOTS, window_cols=cols)["workspaces"]

    def test_every_width_launches_one_row_deep(self):
        """The headline. These fifteen names need three rows at 120 columns and two at 160,
        and a launch gives them one."""
        for cols in (80, 100, 120, 160, 200, 360):
            with self.subTest(cols=cols):
                self.assertEqual(self._rows("f-1", cols), 1)

    def test_a_press_buys_the_rows_the_names_actually_need(self):
        state.record_bar_rows("f-1", 2)
        self.assertEqual(self._rows("f-1", 120), 2)
        state.record_bar_rows("f-1", 3)
        self.assertEqual(self._rows("f-1", 120), 3)

    def test_a_press_on_a_plane_whose_names_fit_changes_nothing(self):
        """**A cap and not a demand**, which is the honest outcome rather than a
        limitation: there is nothing to put on a second row, and drawing a blank one would
        be rows off the harness for a picture."""
        state.record_bar_rows("f-1", 3)
        self.assertEqual(self._rows("f-1", 400), 1)

    def test_a_plane_that_pinned_a_height_still_gets_it_at_launch(self):
        """**#687 stays honoured, and it is the other half of "one row on launch".** A pin
        is a committed decision about how a frame STARTS; the ceiling this issue lowered is
        a default for a plane that committed nothing. `layout._grown` only ever grows, so a
        want of one against a pin of three leaves the three alone — and an operator who
        always wants three rows says so once, in the file where every other layout decision
        already lives.
        """
        with mock.patch.dict(config.FRAME, _placed(workspaces=3)):
            got = commands_frame._slot_sizes("f-1", SLOTS, window_rows=50, pane_cols=120,
                                             order=SLOTS, window_cols=120)
        self.assertEqual(got["workspaces"], 3, got)

    def test_the_height_is_this_frames_and_not_the_planes(self):
        """Two chats of one workspace are two frames on one plane, and the file is per
        frame — so a press in one does not shorten the other's harness."""
        state.record_bar_rows("f-1", 3)
        self.assertEqual(self._rows("f-1", 120), 3)
        self.assertEqual(self._rows("f-2", 120), 1)


class TheKeyRunsTheCommandLive(PersonaIso, unittest.TestCase):
    """`commands_frame.cmd_bar_rows` — what the press actually does to a running frame."""

    def setUp(self):
        super().setUp()
        self.fid = f"br-{_a_dead_pid()}"
        self.enterContext(mock.patch.dict(os.environ,
                                          {"CHARTER_SESSION_ID": self.fid}))
        self.enterContext(mock.patch("charter.frame.tmuxctl.version",
                                     return_value=(3, 7)))
        self.enterContext(mock.patch.dict(config.FRAME, _placed()))
        for name in NAMES:
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
        state.record_harness_pane(self.fid, "%0")
        state.record_panes(self.fid, panels={"top": "%1", "chats": "%3",
                                             "workspaces": "%6", "bottom": "%2",
                                             "repos": "%5", "right": "%4"})

    def _press(self):
        fake = _Tmux()
        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            rc = commands_frame.cmd_bar_rows(SimpleNamespace(chat=""))
        return rc, fake

    def test_a_press_cycles_the_recorded_height(self):
        self.assertIsNone(state.bar_rows(self.fid))
        for want in (2, 3, 1, 2):
            rc, _fake = self._press()
            self.assertEqual(rc, 0)
            self.assertEqual(state.bar_rows(self.fid), want)

    def test_a_press_re_asserts_the_strips_pane_at_its_new_height(self):
        """The number has to reach tmux, or it is the inert value the whole seam is written
        against. `_apply_arrangement` → `_relayout` → `_reassert_sizes` is the one live
        re-layout path, the same one a toggle and a density change go through."""
        _rc, fake = self._press()
        self.assertEqual(fake.resized("%6")[-1], "2", fake.calls)

    def test_a_press_kills_no_pane_and_splits_nothing(self):
        """Nothing is added or removed — the arrangement this frame is drawing is handed
        back unchanged, and what moves is the size."""
        _rc, fake = self._press()
        self.assertEqual([c for c in fake.calls if "kill-pane" in c], [])
        self.assertEqual([c for c in fake.calls if "split-window" in c], [])

    def test_a_press_bumps_the_version_so_the_panels_repaint(self):
        before = state.version(self.fid)
        self._press()
        self.assertGreater(state.version(self.fid), before)

    def test_a_frame_with_no_recorded_harness_pane_is_a_quiet_no_op(self):
        """`_relayout_target`'s refusal, shared with `cmd_toggle` and `cmd_density` so the
        three keypresses cannot come to disagree about what a frame must have before its
        panes may be moved."""
        state.record_harness_pane(self.fid, "not-a-pane-id")
        rc, fake = self._press()
        self.assertEqual(rc, 0)
        self.assertEqual(fake.calls, [])

    def test_outside_a_frame_it_says_so_rather_than_dying_in_a_target(self):
        """`cmd_toggle`'s `if not fid`, and it carries the same consequence: with an empty
        id this command would fall through to `_relayout_target`, where "you are not in a
        frame" and "this frame has no recorded harness pane" are one silence."""
        out = []
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": ""}), \
                mock.patch.object(commands_frame, "outside_a_frame",
                                  side_effect=lambda cmd: out.append(cmd) or 1):
            rc = commands_frame.cmd_bar_rows(SimpleNamespace(chat=""))
        self.assertEqual(rc, 1)
        self.assertEqual(out, ["charter frame-bar-rows"])


class TheKeyIsCharactersOwnAndNotAComponentsToTake(unittest.TestCase):
    """The cost of a third shipped `bind -n`, held down where the other two are."""

    def test_the_bind_reaches_the_config_charter_sources(self):
        text = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=5,
                                        session="s", toggles={})
        line, = [ln for ln in text.splitlines() if "frame-bar-rows" in ln]
        self.assertTrue(line.startswith(f"bind -n {layout.BAR_ROWS_KEY} run-shell "),
                        line)

    def test_the_bind_carries_the_pressers_chat_and_no_client_name(self):
        """One bind text is shared by every frame on `SOCKET`, so which chat pressed comes
        out of the presser's own window (`#{@charter_chat}`) — `frame-toggle`'s twin."""
        text = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=5,
                                        session="s", toggles={})
        line, = [ln for ln in text.splitlines() if "frame-bar-rows" in ln]
        self.assertIn('--chat "#{@charter_chat}"', line)
        self.assertNotIn("-t ", line)

    def test_the_interpreter_is_carried_out_of_band(self):
        """An absolute path re-embedded inside this nested tmux-quote layer is one
        apostrophe away from the silent corruption `commands_frame`'s module docstring
        measures."""
        text = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=5,
                                        session="s", toggles={})
        line, = [ln for ln in text.splitlines() if "frame-bar-rows" in ln]
        self.assertIn('"$CHARTER_PY" -m charter frame-bar-rows', line)
        self.assertNotIn(sys.executable, line)

    def test_it_is_bound_above_the_toggles_and_below_the_hatch(self):
        """**Both ends are load-bearing.** Above the toggles, because tmux has no notion of
        a key conflict — a later `bind -n` replaces an earlier one — so a component
        emitting after charter would silently take the key the refusal below exists to
        protect. Below the hatch, because `overlay.hatch_bind` uses `run-shell -C`, which a
        tmux under `tmuxctl.FLOOR` cannot parse, and everything charter needs must already
        have been applied by the time `source-file` reaches it."""
        text = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=5,
                                        session="s", toggles={"repos": "F7"})
        lines = text.splitlines()
        mine = next(i for i, ln in enumerate(lines) if "frame-bar-rows" in ln)
        toggle = next(i for i, ln in enumerate(lines) if "frame-toggle repos" in ln)
        hatch = next(i for i, ln in enumerate(lines) if overlay.HATCH_KEY in ln)
        self.assertLess(mine, toggle)
        self.assertLess(mine, hatch)

    def test_a_component_may_not_take_the_key(self):
        """`instance.component_arrangement`'s `bound` set, which already holds the palette
        key and the escape hatch. A component that took this one would leave the operator a
        key that cycles nothing and a strip stuck at one row."""
        frame = instance.frame_of({"frame": {"component": [
            {"use": "repos", "key": layout.BAR_ROWS_KEY}]}})
        self.assertEqual(frame["components"], [])
        self.assertEqual(frame["slots"], instance.FRAME_DEFAULTS["slots"])
        # The control: the same table with a free key is placed, so what the line above
        # measures is the collision and not a component charter refused for another reason.
        ok = instance.frame_of({"frame": {"component": [
            {"use": "repos", "key": "F9"}]}})
        self.assertEqual([c["key"] for c in ok["components"]], ["F9"])

    def test_it_is_a_key_that_costs_the_harness_nothing_it_was_using(self):
        """The stated cost of a root-table bind, kept honest: charter claims exactly three
        keys and this is the third, beside the palette's and the escape hatch's. A test
        that only counted would pass on any number; this names them, so a fourth has to be
        argued for here."""
        text = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=5,
                                        session="s", toggles={})
        keys = {ln.split()[2] for ln in text.splitlines() if ln.startswith("bind -n ")}
        self.assertEqual(keys - set(tmuxctl.MOUSE_KEYS),
                         {"F2", overlay.HATCH_KEY, layout.BAR_ROWS_KEY})


class TheStripStillMeasuresItsOwnNames(unittest.TestCase):
    """`slots.bar_rows_wanted`'s contract is unchanged by #880, and that is deliberate:
    the ceiling moved and the measurement did not."""

    def test_the_cap_is_still_the_callers_and_still_bounds_the_answer(self):
        strip = lambda fid: (list(NAMES), "harness-wrapper", "", None)  # noqa: E731
        with mock.patch.dict(slots.BARS, {"probe": strip}):
            answers = [slots.bar_rows_wanted("f-1", "probe", pane_cols=120, cap=c)
                       for c in (1, 2, 3)]
        self.assertEqual(answers, [1, 2, 3])

    def test_a_cap_of_one_is_the_shape_the_launcher_now_asks_for(self):
        """`layout.bar_rows_cap(None)` is 1, so this is what every launch composes — and
        it is the answer the ladder gave at `rows=1` before #829 could grow it."""
        self.assertEqual(layout.bar_rows_cap(state.bar_rows("no-such-frame")), 1)


if __name__ == "__main__":
    unittest.main()
