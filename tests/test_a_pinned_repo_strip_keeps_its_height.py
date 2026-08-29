"""`size` on the repo table's own `[[frame.component]]` table pins the strip's height.

**The default is not changing.** `repos` ships `Content()` and a plane that says nothing
still gets a strip as tall as its clone list — "a two-repo plane gets a two-row strip
rather than a fourteen-row one padded with blanks", which is `layout.repos_rows`' own
docstring and was a deliberate decision. What was missing is the way to *say otherwise*:
an operator whose plane grows and shrinks its clone count wants a strip that stays where
it is, and the `size` key that would have said so was accepted-but-inert — it could only
echo a number charter had already declared, and since `repos` declares no number at all,
**any** `size` on it took the whole arrangement down to `slots` with nothing said.

Three claims are pinned here, and they are separable on purpose:

* **the config boundary** decides which built-ins may carry a `size` that means
  something, and `repos` is the only one — every other placed built-in is `Fixed` in its
  own declaration, `layout` derives that into `SLOT_SIZE` at import, and a per-plane
  number there could only be ignored;
* **the arithmetic** replaces the CONTENT term and leaves the floor and the cap alone, so
  the harness keeps `layout.HARNESS_MIN_ROWS` however large a number was committed —
  measured on tmux 3.7c, an over-large `-y` is not refused, it is granted out of the
  neighbour;
* **the mechanism does not move.** `repos` stays the one variable-row slot and stays out
  of `commands_frame._RESIZE_FLAG`, so it is still the stack's dependent pane and tmux is
  still asked to move exactly N-1 boundaries in an N-pane stack. A pin changes the number
  the other panes are sized around, and nothing about how they are asserted.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import component, layout


def _arrangement(**repos) -> dict:
    """The shipped four, written out, with *repos* merged into the table's own.

    Resolved through `instance.frame_of` rather than assembled by hand: this suite's
    subject is what a committed file resolves to, and a fixture of placements would be a
    second answer that agrees with the boundary until one of them is edited.
    """
    tables = [{"use": "identity"}, {"use": "attention"},
              {"use": "repos", **repos}, {"use": "sidebar"}]
    return instance.frame_of({"frame": {"component": tables}})


#: The split order every case below uses — charter's own, and the one whose arithmetic
#: `tests/test_frame_layout.py` already pins for the unpinned case.
SLOTS = ["top", "bottom", "repos", "right"]


class TheConfigBoundaryDecidesWhichBuiltInsMayCarryASize(unittest.TestCase):
    """A committed number is honoured exactly where something reads it.

    Not "on the components charter feels like allowing it on": the rule is mechanical and
    is the same one `edge` already keeps. `layout._derive` turns each component's declared
    size into `SLOT_SIZE` once, at import, and `_size_of` answers `top`, `bottom` and
    `right` out of that table forever after — so a number on those three has nowhere to be
    read. `repos` is `Content()`, which never enters that table: `slot_sizes` routes it to
    `repos_rows`, recomputed from the resolved arrangement at every launch and again on
    every `window-resized`.
    """

    def test_the_repo_table_resolves_a_committed_size_to_a_fixed_policy(self):
        """The placement is where the number survives to, so it is asserted there and not
        only in the rows that come out later — a boundary that accepted the key and then
        dropped it would be the inert `size` this change exists to remove, one layer
        further in."""
        placed = _arrangement(size=15)["components"]
        self.assertEqual([p["size"] for p in placed],
                         [component.Fixed(1), component.Fixed(1),
                          component.Fixed(15), component.Fixed(22)])

    def test_a_table_that_names_no_size_keeps_the_policy_the_component_declares(self):
        """Writing the arrangement out is not the same as pinning it. `Content()` here is
        the SHIPPED policy spelled longhand — which is what charter's own `charter.toml`
        commits — and reading it as a pin would hand every such plane a one-row strip."""
        placed = _arrangement()["components"]
        self.assertEqual([p["size"] for p in placed],
                         [component.Fixed(1), component.Fixed(1),
                          component.Content(), component.Fixed(22)])

    def test_the_shorthand_spellings_place_the_component_at_its_own_size(self):
        """`slots` and `density` have no place to write a number, so both must resolve to
        the declared policy. Asserted through `frame_components`, which is the one place
        all three spellings become one arrangement."""
        for cfg in ({}, {"frame": {"slots": SLOTS}}, {"frame": {"density": "full"}}):
            with self.subTest(cfg=cfg):
                got = instance.frame_components(cfg)
                repos, = [p for p in got if p["slot"] == "repos"]
                self.assertEqual(repos["size"], component.Content())

    def test_a_built_in_whose_geometry_is_derived_at_import_may_still_only_echo(self):
        """The other side of the asymmetry, and the half that did NOT change.

        Each pair is the same component with the number it declares and with one it does
        not, so an accepted echo cannot be what makes the refusal look right. `True` is in
        the list because `True == 1` in Python and both strips are `Fixed(1)`: without the
        explicit `bool` check it would compare equal and be accepted as a number nobody
        wrote.
        """
        for use, echo, other in (("identity", 1, 2), ("attention", 1, True),
                                 ("sidebar", 22, 30)):
            with self.subTest(use=use):
                kept = instance.component_tables({"component": [{"use": use,
                                                                "size": echo}]})
                self.assertEqual([p["size"] for p in kept],
                                 [component.Fixed(echo)])
                self.assertIsNone(instance.component_tables(
                    {"component": [{"use": use, "size": other}]}))

    def test_an_arrangement_with_a_pin_round_trips_through_the_written_out_form(self):
        """`frame_components` promises the mapping is lossless in both directions, and a
        pin is the first value that mapping has ever had to carry which is NOT the
        component's own declaration. Resolve it, write it back out, resolve that."""
        first = _arrangement(size=15)["components"]
        tables = [{"use": p["use"], "edge": p["edge"], "size": p["size"].n}
                  for p in first]
        again = instance.frame_of({"frame": {"component": tables}})["components"]
        self.assertEqual(again, first)


class ThePinnedStripIsTheHeightTheOperatorCommitted(unittest.TestCase):
    """What the operator asked for: a strip that does not move with the clone count."""

    def test_the_strip_is_the_committed_height_whatever_its_content_wants(self):
        """The whole complaint, in one assertion: *"when less repos — its very small"*.
        Every one of these content counts is a real answer `slots.repos_rows_wanted` gives
        on some plane — none, one, a couple, a full table, more than the window has — and
        the pinned strip is the same height for all of them."""
        with mock.patch.dict(config.FRAME, _arrangement(size=15)):
            for content in (0, 1, 2, 14, 30):
                with self.subTest(content=content):
                    self.assertEqual(
                        layout.repos_rows(content_rows=content, window_rows=50,
                                          slots=SLOTS),
                        15)

    def test_a_plane_that_pins_nothing_is_sized_by_its_content_exactly_as_before(self):
        """The default, asserted against a written-out arrangement rather than against a
        `slots` list — because a written-out arrangement is where a `repos` placement
        exists to be misread, and charter's own plane commits one. Two counts, so a pin
        read off the wrong placement (`identity` is `Fixed(1)` and comes first in file
        order) cannot pass by coincidence."""
        with mock.patch.dict(config.FRAME, _arrangement()):
            for content in (4, 7):
                with self.subTest(content=content):
                    self.assertEqual(
                        layout.repos_rows(content_rows=content, window_rows=50,
                                          slots=SLOTS),
                        content)

    def test_the_pin_reaches_tmux_as_the_length_the_pane_is_split_to(self):
        """The seam that makes any of this visible: `slot_sizes` into `panel_argvs` into
        `-l`. Asserted as the literal string tmux would see, so a number computed
        correctly and then dropped on the way out is red."""
        with mock.patch.dict(config.FRAME, _arrangement(size=15)):
            sizes = layout.slot_sizes(SLOTS, window_rows=50, content_rows=2)
            cmds = layout.panel_argvs(slots=SLOTS, sizes=sizes, session="f-1",
                                      socket="charter", harness_pane="%0")
        self.assertEqual(sizes, {"top": 1, "bottom": 1, "repos": 15, "right": 22})
        repos = cmds[SLOTS.index("repos")]
        self.assertEqual(repos[repos.index("-l") + 1], "15")

    def test_the_launcher_sizes_the_pane_from_the_pin_and_not_from_the_clone_count(self):
        """`commands_frame._launch_sizes` is the launch-time half, and it is the caller
        that reads the plane's real clone count. Stubbing `repos_rows_wanted` to a number
        nothing else in this test could produce is what makes the assertion about which
        of the two won."""
        with mock.patch.dict(config.FRAME, _arrangement(size=15)), \
                mock.patch("charter.frame.slots.repos_rows_wanted", return_value=3):
            got = commands_frame._launch_sizes("f-1", SLOTS,
                                               window_cols=200, window_rows=50)
        self.assertEqual(got["repos"], 15)

    def test_the_harness_takes_exactly_the_rows_the_pinned_strip_leaves(self):
        """`harness_rows` is the number `_reassert_sizes` asserts on the harness itself,
        and the pinned strip is still the pane left to take the remainder. So the two have
        to add up to the window with nothing over: every strip, its border, and the
        harness. A pin the harness arithmetic did not know about would show up here as
        rows that belong to nobody."""
        rows = 50
        with mock.patch.dict(config.FRAME, _arrangement(size=15)):
            sizes = layout.slot_sizes(SLOTS, window_rows=rows, content_rows=2)
            harness = layout.harness_rows(sizes, window_rows=rows)
        strips = sum(n + layout._BORDER_ROWS for slot, n in sizes.items()
                     if slot != "right")
        self.assertEqual(strips + harness, rows)
        self.assertGreaterEqual(harness, layout.HARNESS_MIN_ROWS)


class APinChangesTheNumberAndNotTheMechanism(unittest.TestCase):
    """The design refutation this change was asked to consider, pinned as a property.

    The obvious way to make `size` work is to call a pinned `repos` a FIXED row — derive
    `VARIABLE_ROW_SLOTS` from the resolved arrangement instead of from the shipped
    registry. Two things are wrong with it. `slot_sizes` would then answer `repos` from
    `_size_of`, which reads `SLOT_SIZE["repos"]` — the shipped FLOOR, `1`, not the
    committed number — so the pinned strip would come out one row tall. And it would buy
    nothing at the tmux end: `_RESIZE_FLAG` has no `repos` entry, adding one would assert
    N heights in an N-pane stack, and that is the measured failure (`top`, `bottom`,
    `repos` in split order at 200x50 left the table 1 row and the strip 6 — the two sizes
    swapped panes). The strip has to stay the dependent pane whether its height is
    content-derived or committed.
    """

    def test_the_table_is_still_the_one_variable_row_slot_when_a_plane_pins_it(self):
        """Derived from the SHIPPED registry, and it stays that way. `_variable_pane_cols`
        picks the pane to measure with a `next()` over this set and its own comment says
        there is exactly one member by construction; an empty set there would silently
        fall back to a derivation, and a pin must not be what makes that happen."""
        with mock.patch.dict(config.FRAME, _arrangement(size=15)):
            self.assertEqual(layout.VARIABLE_ROW_SLOTS, frozenset({"repos"}))
            self.assertIs(layout._is_fixed_row("repos"), False)

    def test_the_pinned_strip_is_still_the_pane_tmux_is_never_told_the_height_of(self):
        """In a stack of N panes only N-1 heights are free. `_RESIZE_FLAG` names the
        three whose size is a constant; the strip lands on its number because everything
        else was asserted around it, which is the same mechanism a content-sized strip
        already uses."""
        self.assertNotIn("repos", commands_frame._RESIZE_FLAG)
        self.assertEqual(sorted(commands_frame._RESIZE_FLAG), ["bottom", "right", "top"])

    def test_a_pin_does_not_move_the_shipped_geometry_tables(self):
        """`SLOT_SIZE["repos"]` is the FLOOR and stays it: `repos_rows` reads it as the
        floor, and `panel_argvs` falls back to it for a caller with no window to measure.
        A pin that had edited this table would change both of those for a plane that never
        asked."""
        with mock.patch.dict(config.FRAME, _arrangement(size=15)):
            self.assertEqual(layout.SLOT_SIZE["repos"], 1)
            self.assertEqual(layout._size_of("repos"), 1)


class APinIsStillCappedSoTheSessionKeepsItsFloor(unittest.TestCase):
    """The one measured safety property a committed number must not be able to lift.

    tmux does not refuse an over-large height — it grants it out of the neighbour. On
    3.7c, `resize-pane -t <the table pane> -y 40` in a 20-row window left the HARNESS pane
    one row tall. A committed `size = 40` is that command with a config file in front of
    it, and a plane's frame is committed and shared, so it would arrive on a laptop whose
    terminal its author never saw.
    """

    def test_a_pin_larger_than_the_window_can_spare_is_cut_to_what_it_can(self):
        """Asserted as the HARNESS's remaining rows rather than as a literal height, so
        the arithmetic is checked rather than restated — the shape
        `tests/test_frame_layout.py` already uses for the content-sized cap."""
        rows = 24
        with mock.patch.dict(config.FRAME, _arrangement(size=15)):
            got = layout.repos_rows(content_rows=2, window_rows=rows, slots=SLOTS)
        harness = (rows - got - layout.SLOT_SIZE["top"] - layout.SLOT_SIZE["bottom"]
                   - 3 * layout._BORDER_ROWS)
        self.assertGreaterEqual(harness, layout.HARNESS_MIN_ROWS)
        self.assertLess(got, 15, "the cap did not bind at all")

    def test_the_cap_is_the_same_arithmetic_a_content_sized_strip_gets(self):
        """One rule, not two. A pin replaces what the strip WANTS; what the window can
        spare is decided afterwards and identically, which is what stops a second, weaker
        copy of `HARNESS_MIN_ROWS` growing behind the new key."""
        for rows in (18, 20, 24, 30, 50):
            with self.subTest(rows=rows):
                with mock.patch.dict(config.FRAME, _arrangement(size=99)):
                    pinned = layout.repos_rows(content_rows=0, window_rows=rows,
                                               slots=SLOTS)
                with mock.patch.dict(config.FRAME, _arrangement()):
                    grown = layout.repos_rows(content_rows=99, window_rows=rows,
                                              slots=SLOTS)
                self.assertEqual(pinned, grown)

    def test_the_floor_holds_when_the_window_has_no_rows_to_spare_at_all(self):
        """Below the floor is a zero or negative `-l`, which tmux refuses outright — the
        frame would come up with no strip at all in exactly the terminal least able to
        afford a missing panel. What protects the harness there is `visible_slots`, which
        drops the slot rather than shrinking it."""
        with mock.patch.dict(config.FRAME, _arrangement(size=15)):
            self.assertEqual(
                layout.repos_rows(content_rows=0, window_rows=6, slots=SLOTS),
                layout.SLOT_SIZE["repos"])
