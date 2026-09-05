"""#903 item 1: a tab strip an operator drags taller stays taller.

*"when switching between workspaces — resized tabs resetting to one line. it should
preserve sizes."*

**Measured on the reporting plane: no frame had a recorded `bar_rows` at all.** So the
gesture was a tmux pane drag rather than `layout.BAR_ROWS_KEY`, and `layout._grown`
recomputes the height on the next layout pass — which a workspace switch triggers
(`_switch_client` → `_apply_arrangement` → `_relayout` → `_reassert_sizes`). The drag was
discarded in silence. #880 chose the keybinding over the drag *because* the layout owns
bar heights and judged teaching it otherwise "a much larger change than the one-row
default"; what that left is the obvious gesture failing quietly and a key nobody knows
about as the only way to do the thing.

**The hazard is the whole difficulty, and it is what every case below is arranged
around: charter resizes these panes itself.** Adopting charter's own resize as "the
operator chose this" would pin whatever the layout last computed and freeze the strip
forever — the exact opposite defect, and a worse one, because it cannot be undone by
dragging back. So the comparison is against **what charter last asked for**
(`state.record_asserted_bars`, written by `_reassert_sizes` at the end of every pass),
never against the previous height. `CharterOwnResizeIsNotAGesture` is the half that says
so.

**One meaning, one file.** A drag writes `state.record_bar_rows` — the same value `F3`
records — so there is one place to look for "how tall are this frame's strips" and one
rule for what that number means: it is a CEILING, and `slots.bar_rows_wanted` still holds
each strip to the rows its own names fill.

No tmux here. `tests/test_a_real_resize_gives_a_real_strip_another_row.py` is where a real
server is asked; this is charter's arithmetic on a box with none.
"""

from __future__ import annotations

import subprocess
import unittest
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import layout, slots, state

from tests import _tmuxchain
from tests._isolation import PersonaIso

FID = "f-1"

#: A frame that places the workspaces strip, the attention strip and the table, in split
#: order. Both a BAR and two non-bars, so a case can say which slots the record carries.
SLOTS = ["top", "workspaces", "bottom", "repos"]

#: Slot → pane, in that same order, which is the shape `state.panes` round-trips.
PANES = {"top": "%1", "workspaces": "%2", "bottom": "%3", "repos": "%4"}


def _placed():
    """The resolved arrangement that places the workspaces bar, built the way an
    operator's `charter.toml` reaches `config.FRAME` — through `instance.frame_of`.

    A bar has to be PLACED for `layout` to have any geometry for it at all: neither bar is
    in `builtins.SLOT_OF`, so `layout._size_of` answers for one only out of a committed
    arrangement (#687), and a case that patched none would be asserting about a slot that
    is not in the answer.
    """
    return instance.frame_of({"frame": {"component": [
        {"use": "identity"}, {"use": "workspaces"}, {"use": "attention"},
        {"use": "repos"}]}})


class _AFrameThatHasBeenSized(PersonaIso, unittest.TestCase):
    """A frame whose panes exist, with tmux replaced by a fake that answers `list-panes`
    with whatever heights the case says the panes have."""

    #: What the strip's names need, stubbed so the arithmetic below is about the CEILING
    #: rather than about how many workspaces this fixture happened to make. Three is
    #: `layout.BAR_MAX_ROWS`, so a raised ceiling is always spendable.
    WANTS = 3

    def setUp(self):
        super().setUp()
        state.frame_dir(FID, create=True)
        self.heights: dict[str, int] = {}
        self.calls: list[list[str]] = []
        self.enterContext(mock.patch.dict(config.FRAME, _placed()))
        self.enterContext(mock.patch.object(slots, "bar_rows_wanted",
                                            side_effect=self._wanted))
        self.enterContext(mock.patch.object(slots, "repos_rows_wanted", return_value=3))
        self.enterContext(mock.patch("charter.frame.tmuxctl.run", side_effect=self._run))

    def _wanted(self, _fid, slot, *, pane_cols, cap):
        """`slots.bar_rows_wanted`, stubbed — and it OBSERVES THE CAP, which is the half a
        `return_value=3` would have thrown away.

        The real one composes the strip at 1, 2 … *cap* and keeps the tallest height it
        fills, so the ceiling is what turns a frame that has chosen nothing into a one-row
        strip. A stub that ignored it would make every case here start from a three-row
        strip and there would be nothing for a drag to change.
        """
        return min(self.WANTS, cap) if slot in slots.BARS else 1

    def _run(self, action, argv, *, env=None, timeout=None, report=True):
        """tmux, faked: `list-panes` answers this case's heights and everything else is a
        write that succeeds and is recorded."""
        self.calls.extend(_tmuxchain.commands(argv))
        if "list-panes" in argv:
            out = "".join(f"{PANES[s]}:{n}\n" for s, n in self.heights.items())
            return subprocess.CompletedProcess(argv, 0, stdout=out, stderr="")
        if "display-message" in argv:
            return subprocess.CompletedProcess(argv, 0, stdout="200\n", stderr="")
        return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")

    def _resize(self, *, window_rows=50, panes=None):
        """One `window-resized` pass over this frame — the one place charter states its
        intent about a pane height, and therefore the one place a drag can be noticed."""
        commands_frame._reassert_sizes(
            "sock", fid=FID, panes=dict(panes if panes is not None else PANES),
            harness_pane="%0", window_cols=200, window_rows=window_rows)

    def _asserted(self) -> dict[str, int]:
        """The heights charter told tmux to make each pane, off the fake's own argv."""
        return {c[c.index("-t") + 1]: int(c[c.index("-y") + 1])
                for c in self.calls if "resize-pane" in c and "-y" in c}

    def _settle(self, *, window_rows=50):
        """Two passes with nothing moving in between, leaving the frame in the state a
        running frame is ordinarily in: an intent recorded, and the panes standing at it.
        """
        self._resize(window_rows=window_rows)
        asserted = self._asserted()
        self.heights = {s: asserted[p] for s, p in PANES.items() if p in asserted}
        self.calls.clear()


class WhatCharterAskedForIsWrittenDown(_AFrameThatHasBeenSized):
    """`state.record_asserted_bars` — the reference point, and the three things it has to
    carry for the comparison to mean anything."""

    def test_a_resize_records_the_height_it_asked_each_strip_to_be(self):
        self._resize()
        was = state.asserted_bars(FID)
        self.assertEqual(was.rows, {"workspaces": self._asserted()[PANES["workspaces"]]})

    def test_only_the_strips_are_in_it_and_not_every_pane(self):
        """The record answers "what did charter ask the STRIPS to be". `repos` is the
        stack's dependent pane and is never asserted at all; `top` and `bottom` are
        constants nothing adopts. A record of every slot would invite a later reader to
        adopt a drag on a pane that has no `bar_rows` to write."""
        self._resize()
        self.assertEqual(set(state.asserted_bars(FID).rows), {"workspaces"})

    def test_the_window_and_the_slot_set_ride_with_it(self):
        """Both are what let the next pass tell a hand from tmux's own redistribution —
        see `_adopt_dragged_bars` for the two paths each one closes."""
        self._resize(window_rows=44)
        was = state.asserted_bars(FID)
        self.assertEqual(was.window_rows, 44)
        self.assertEqual(was.panes, tuple(sorted(PANES)))

    def test_a_frame_with_no_state_directory_records_nothing(self):
        """**The resize path must not MINT state for a frame that has none.** It runs on
        every `window-resized` for a frame whose panes are already recorded, so the
        directory is there by construction — and a writer that created it would let the
        hot path resurrect a frame `state.reap` had removed, which is the failure
        `state.frame_dir`'s own `create=False` default exists to prevent for readers.
        """
        import shutil
        shutil.rmtree(state.frame_dir(FID))
        self._resize()
        self.assertEqual(state.asserted_bars(FID).rows, {})
        self.assertFalse(state.frame_dir(FID).exists(),
                         "a resize made a state directory for a frame that had none")

    def test_every_pass_records_even_when_it_adopts_nothing(self):
        """A pass that skipped the record would leave the NEXT one comparing against an
        intent two resizes old — which is a difference explained by the resize in
        between and not by any hand."""
        self._resize(window_rows=50)
        self._resize(window_rows=30)
        self.assertEqual(state.asserted_bars(FID).window_rows, 30)


class CharterOwnResizeIsNotAGesture(_AFrameThatHasBeenSized):
    """**The hazard, and the reason the comparison is against the intent rather than the
    previous height.** charter re-asserts these panes on every `window-resized` and every
    re-layout. A rule that adopted whatever it found would pin the strip at the layout's
    own last answer and never let go of it."""

    def test_a_frame_standing_exactly_where_charter_put_it_records_no_height(self):
        self._settle()
        self._resize()
        self.assertIsNone(state.bar_rows(FID),
                          "charter adopted its own resize as the operator's choice")

    def test_ten_resizes_that_move_nothing_still_record_no_height(self):
        """The freeze this prevents is cumulative: it takes one adoption to pin the strip
        forever, so a rule that is merely usually right is not right."""
        self._settle()
        for _ in range(10):
            self._resize()
        self.assertIsNone(state.bar_rows(FID))

    def test_a_window_that_changed_rows_adopts_nothing_and_asks_tmux_nothing(self):
        """tmux rescales every pane proportionally BEFORE anything charter runs, so on a
        real window resize every height differs from the intent and none of them was
        dragged. Refused before the measurement, which is what keeps a terminal drag —
        where the rows change at every step — costing no extra tmux call at all."""
        self._settle()
        self.heights = {"workspaces": 3, "top": 1, "bottom": 1, "repos": 3}
        self.calls.clear()
        self._resize(window_rows=40)
        self.assertIsNone(state.bar_rows(FID))
        self.assertEqual([c for c in self.calls if "list-panes" in c], [])

    def test_a_frame_whose_shape_changed_adopts_nothing(self):
        """A re-layout kills and splits panes and tmux rescales the SURVIVORS when it
        does — and `_relayout` hands this only the panes it KEPT. A height that differs
        there is explained by the move."""
        self._settle()
        self.heights["workspaces"] = 3
        self.calls.clear()
        self._resize(panes={s: p for s, p in PANES.items() if s != "repos"})
        self.assertIsNone(state.bar_rows(FID))

    def test_a_frame_that_places_no_bar_asks_tmux_nothing_at_all(self):
        """**The commonest frame on the plane, and it must not pay for this feature.**
        charter places neither bar by default, so the record's `rows` map is empty for
        nearly every frame there is — and an empty map has nothing a drag could differ
        from. Refused before the read, so the ordinary frame's resize costs exactly what
        it cost before #903.
        """
        bare = {s: p for s, p in PANES.items() if s != "workspaces"}
        self._resize(panes=bare)
        self.assertEqual(state.asserted_bars(FID).rows, {})
        self.calls.clear()
        self._resize(panes=bare)
        self.assertEqual([c for c in self.calls if "list-panes" in c], [])

    def test_the_first_pass_of_a_frames_life_adopts_nothing(self):
        """There is no intent for a height to differ from yet, so there is nothing to
        measure against — and nothing is measured."""
        self.heights = {"workspaces": 3, "top": 1, "bottom": 1, "repos": 3}
        self._resize()
        self.assertIsNone(state.bar_rows(FID))
        self.assertEqual([c for c in self.calls if "list-panes" in c], [])


class ADragIsAdoptedAsThisFramesChoice(_AFrameThatHasBeenSized):
    """The gesture working, which is what #903 was filed for."""

    def test_a_strip_dragged_taller_is_recorded_at_the_height_it_was_dragged_to(self):
        self._settle()
        self.heights["workspaces"] = 3
        self._resize()
        self.assertEqual(state.bar_rows(FID), 3)

    def test_a_strip_dragged_shorter_is_recorded_too(self):
        """**Both directions, and the second is what makes it an adoption rather than a
        ratchet.** An operator who drags a strip back down has said something as clearly
        as the one who dragged it up, and a rule that only ever grew would leave them
        pressing `F3` three times to undo a gesture."""
        state.record_bar_rows(FID, 3)
        self._settle()
        self.heights["workspaces"] = 1
        self._resize()
        self.assertEqual(state.bar_rows(FID), 1)

    def test_the_height_reaches_the_pane_on_the_very_next_pass(self):
        """The adoption happens before `_slot_sizes` is called, which is what reads
        `state.bar_rows`. Written after it instead, the operator's own drag would take a
        second whole resize to appear — and on the switch path there is no second one."""
        self._settle()
        self.assertEqual(self.heights["workspaces"], 1,
                         "the fixture did not start from a one-row strip")
        self.heights["workspaces"] = 3
        self.calls.clear()
        self._resize()
        self.assertEqual(self._asserted()[PANES["workspaces"]], 3)

    def test_a_drag_and_the_key_write_the_same_file(self):
        """#903's own requirement for the gesture: *"writing the same stored value `F3`
        records, so both gestures have one meaning and one place to look."* Asserted by
        driving the key's own recorder to the same number and finding nothing to
        distinguish."""
        self._settle()
        self.heights["workspaces"] = 2
        self._resize()
        by_drag = state.bar_rows(FID)
        state.record_bar_rows(FID, layout.next_bar_rows(None))
        self.assertEqual(by_drag, state.bar_rows(FID))

    def test_the_taller_of_two_dragged_strips_is_what_the_frame_records(self):
        """`state.bar_rows` is the frame's ceiling and both strips share it. An operator
        who dragged one of two bars taller has asked for that much room; the other is
        still held to the rows its own names fill."""
        self._settle()
        self.heights["workspaces"] = 3
        self.heights["bottom"] = 2
        with mock.patch.object(state, "asserted_bars", return_value=state.AssertedBars(
                50, tuple(sorted(PANES)), {"workspaces": 1, "bottom": 1})):
            self._resize()
        self.assertEqual(state.bar_rows(FID), 3)

    def test_a_height_already_recorded_is_not_rewritten(self):
        """A `bar_rows` file rewritten with its own contents on every resize is an
        `os.replace` per drag step for no change."""
        self._settle()
        self.heights["workspaces"] = 3
        self._resize()
        was = (state.frame_dir(FID) / "bar_rows").stat().st_mtime_ns
        self._settle()
        self.heights["workspaces"] = 3
        self._resize()
        self.assertEqual((state.frame_dir(FID) / "bar_rows").stat().st_mtime_ns, was)


class TheRecordIsReadTheWayEveryOtherFileHereIs(PersonaIso, unittest.TestCase):
    """`state.asserted_bars` — shape-checked on the way out, for `state.panes`' reason.

    This file is JSON on disk, so a truncated write or a hand edit reaches the reader as a
    plausible-looking map; what comes out of it decides whether charter writes a height
    into `bar_rows`, so a string where an integer belongs must be "nothing recorded"
    rather than a `TypeError` on the sizing path.
    """

    def setUp(self):
        super().setUp()
        state.frame_dir(FID, create=True)

    def _wrote(self, text: str):
        (state.frame_dir(FID) / "asserted_bars").write_text(text)
        return state.asserted_bars(FID)

    def test_a_frame_nothing_has_been_asserted_for_reads_as_an_empty_record(self):
        """And the window is `0`, which is not a window any measurement can equal — so the
        sentinel cannot pass for a real one in the caller's own comparison."""
        was = state.asserted_bars(FID)
        self.assertEqual((was.window_rows, was.panes, was.rows), (0, (), {}))

    def test_a_round_trip_answers_what_was_written(self):
        state.record_asserted_bars(FID, window_rows=50, panes=["top", "workspaces"],
                                   rows={"workspaces": 2})
        was = state.asserted_bars(FID)
        self.assertEqual((was.window_rows, was.panes, was.rows),
                         (50, ("top", "workspaces"), {"workspaces": 2}))

    def test_the_panes_are_sorted_so_the_comparison_is_about_the_SET(self):
        """The caller compares against `sorted(panes)`, and both callers hand it a map
        whose insertion order is the split order — which a re-layout permutes without
        changing which panes there are."""
        state.record_asserted_bars(FID, window_rows=50, panes=["repos", "top"], rows={})
        self.assertEqual(state.asserted_bars(FID).panes, ("repos", "top"))

    def test_every_shape_a_hand_edit_can_produce_reads_as_nothing_recorded(self):
        for text in ('not json at all', '[]', '"a string"', '{}',
                     '{"window_rows": "50", "panes": [], "rows": {}}',
                     '{"window_rows": 50, "panes": [], "rows": []}',
                     '{"window_rows": 50, "rows": {}}',
                     '{"window_rows": 50, "panes": {}, "rows": {}}'):
            with self.subTest(text=text):
                was = self._wrote(text)
                self.assertEqual((was.window_rows, was.panes, was.rows), (0, (), {}))

    def test_a_member_of_the_wrong_type_is_dropped_and_the_rest_kept(self):
        was = self._wrote('{"window_rows": 50, "panes": ["top", 7], '
                          '"rows": {"workspaces": 2, "chats": "two"}}')
        self.assertEqual(was.panes, ("top",))
        self.assertEqual(was.rows, {"workspaces": 2})

    def test_a_write_that_cannot_complete_is_a_no_op_and_never_an_exception(self):
        """This runs inside the `frame-resize` child, on the path that puts every panel
        back where it belongs. A full filesystem must cost the frame its next adoption and
        nothing else — `record_panes`' own silence, for its own reason."""
        with mock.patch.object(state.config, "replace_for",
                               side_effect=OSError("no space")):
            state.record_asserted_bars(FID, window_rows=50, panes=[], rows={})
        self.assertEqual(state.asserted_bars(FID).window_rows, 0)

    def test_an_id_that_cannot_name_a_directory_writes_nothing_and_does_not_raise(self):
        """`fid` reaches `_reassert_sizes` off a tmux hook's argv, so it is untrusted the
        way every other id charter joins onto a path is (#442). `frame_dir` refuses it and
        this writes nothing, rather than raising out of the resize child."""
        state.record_asserted_bars("../evil", window_rows=50, panes=[], rows={})
        self.assertEqual(state.asserted_bars("../evil").window_rows, 0)


class ADragOutsideTheRangeIsClampedAndNotDiscarded(unittest.TestCase):
    """`layout.adopted_bar_rows` — and why it clamps where `layout.bar_rows_cap`
    degrades."""

    def test_a_drag_past_the_ceiling_takes_the_ceiling(self):
        """An operator who dragged a strip to eight rows asked for as many as they can
        have. Answering that with `BAR_ROWS_DEFAULT` — which is what `bar_rows_cap` does
        for a value off disk — is the silent discard #903 was filed about."""
        self.assertEqual(layout.adopted_bar_rows(8), layout.BAR_MAX_ROWS)

    def test_a_drag_below_the_floor_takes_the_floor(self):
        """The opposite input and the opposite bound. Not reachable through tmux, which
        gives no pane zero rows, and clamped rather than guarded separately for that
        reason: one expression answers for both ends."""
        self.assertEqual(layout.adopted_bar_rows(0), layout.BAR_ROWS_DEFAULT)

    def test_every_height_inside_the_range_is_itself(self):
        for rows in range(layout.BAR_ROWS_DEFAULT, layout.BAR_MAX_ROWS + 1):
            self.assertEqual(layout.adopted_bar_rows(rows), rows)

    def test_the_file_it_writes_is_the_one_the_ceiling_is_read_from(self):
        """A clamp that produced a number `layout.bar_rows_cap` then degraded would be a
        drag that recorded a height and changed nothing — the failure with an extra file
        write in it."""
        for rows in (0, 1, 2, 3, 8, 4000):
            with self.subTest(rows=rows):
                chose = layout.adopted_bar_rows(rows)
                self.assertEqual(layout.bar_rows_cap(chose), chose)


class ThePaneHeightsAreTmuxsAnswer(_AFrameThatHasBeenSized):
    """`commands_frame._pane_rows` — one read, joined on the recorded map."""

    def test_it_asks_once_for_the_whole_window(self):
        """`list-panes` and not one `display-message` per strip: this runs on the
        `window-resized` path, where one round trip is ~5ms and dominated by spawning the
        tmux client."""
        self._settle()
        self.heights["workspaces"] = 3
        self.calls.clear()
        self._resize()
        self.assertEqual(len([c for c in self.calls if "list-panes" in c]), 1)

    def test_a_pane_tmux_reports_that_charter_did_not_record_joins_nothing(self):
        """The harness pane is in the window and is not a slot; so is anything an operator
        split by hand. Which pane is which is `state.panes`' answer and never a position."""
        with mock.patch("charter.frame.tmuxctl.run", side_effect=lambda *a, **k:
                        subprocess.CompletedProcess(
                            [], 0, stdout="%0:41\n%2:2\n%99:7\n", stderr="")):
            self.assertEqual(commands_frame._pane_rows("sock", panes=PANES),
                             {"workspaces": 2})

    def test_a_line_that_does_not_parse_contributes_nothing(self):
        with mock.patch("charter.frame.tmuxctl.run", side_effect=lambda *a, **k:
                        subprocess.CompletedProcess(
                            [], 0, stdout="%2:two\n%3:1\nnonsense\n", stderr="")):
            self.assertEqual(commands_frame._pane_rows("sock", panes=PANES),
                             {"bottom": 1})

    def test_a_failed_read_is_no_heights_and_no_raise(self):
        """A timeout folds into a return code (`tmuxctl.TIMED_OUT`) and hands back
        whatever the killed process had written, so a partial read can parse. The return
        code is what decides."""
        with mock.patch("charter.frame.tmuxctl.run", side_effect=lambda *a, **k:
                        subprocess.CompletedProcess([], 1, stdout="%2:9\n", stderr="")):
            self.assertEqual(commands_frame._pane_rows("sock", panes=PANES), {})

    def test_a_pane_id_that_is_not_tmuxs_own_shape_never_reaches_an_argv(self):
        """#475's rule on the target too: it comes off disk (`state.panes`) and goes
        straight into a `-t`."""
        asked = []
        with mock.patch("charter.frame.tmuxctl.run", side_effect=lambda a, argv, **k:
                        asked.append(argv) or subprocess.CompletedProcess(
                            argv, 0, stdout="", stderr="")):
            self.assertEqual(
                commands_frame._pane_rows("sock", panes={"top": "; kill-server"}), {})
        self.assertEqual(asked, [])


if __name__ == "__main__":
    unittest.main()
