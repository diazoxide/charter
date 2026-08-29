"""`size` on the repo table's own `[[frame.component]]` table pins the strip's height.

**The default is not changing.** `repos` ships `Content()` and a plane that says nothing
still gets a strip as tall as its clone list — "a two-repo plane gets a two-row strip
rather than a fourteen-row one padded with blanks", which is `layout.repos_rows`' own
docstring and was a deliberate decision. What was missing is the way to *say otherwise*:
an operator whose plane grows and shrinks its clone count wants a strip that stays where
it is, and the `size` key that would have said so was accepted-but-inert — it could only
echo a number charter had already declared, and since `repos` declares no number at all,
**any** `size` on it took the whole arrangement down to `slots` with nothing said.

Four claims are pinned here, and they are separable on purpose:

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
  the other panes are sized around, and nothing about how they are asserted;
* **the plane is read at a boundary and the arithmetic is handed the answer** (#661).
  `layout.repos_rows` is charter's one provably pure geometry function and its tests are
  written to that property; the first cut of this feature read `config.FRAME` from inside
  it, so `repos_rows(content_rows=4, window_rows=50, slots=[...])` answered `15` with
  neither bound binding, out of a file the caller never named. On this repository
  `charter.toml` is tracked, so an operator following this feature's own news entry
  turned six tests red for everyone. `commands_frame._slot_sizes` is the boundary now and
  `layout.pinned_repo_rows` is the read.
"""

from __future__ import annotations

import pathlib
import subprocess
import tomllib
import unittest
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import builtins, component, layout

#: This repository's own committed plane file. Read rather than described, for
#: `test_component_id_is_the_currency.CharterOwnConfigIsUnchanged`'s reason and for one
#: more of its own: #661 was a defect an operator triggered by editing exactly this file.
_COMMITTED = pathlib.Path(__file__).resolve().parents[1] / "charter.toml"


def _arrangement(**repos) -> dict:
    """The shipped four, written out, with *repos* merged into the table's own.

    Resolved through `instance.frame_of` rather than assembled by hand: this suite's
    subject is what a committed file resolves to, and a fixture of placements would be a
    second answer that agrees with the boundary until one of them is edited.
    """
    tables = [{"use": "identity"}, {"use": "attention"},
              {"use": "repos", **repos}, {"use": "sidebar"}]
    return instance.frame_of({"frame": {"component": tables}})


def _pin(**repos) -> int | None:
    """What `layout.pinned_repo_rows` reads out of the arrangement *repos* describes.

    **The seam, called as the launcher calls it, and never a literal.** The number the
    arithmetic is handed below is this function's answer and not a `15` written twice, so
    a boundary that stopped resolving `size`, or a reader that stopped finding it, is red
    in every case that depends on the pin rather than only in the one that asserts the
    read. `config.FRAME` is patched HERE and only here, which is the whole shape #661
    asked for: the plane is read at a boundary and its answer is passed down.
    """
    with mock.patch.dict(config.FRAME, _arrangement(**repos)):
        return layout.pinned_repo_rows()


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
    `repos_rows`, which is handed the resolved arrangement's number at every launch and
    again on every `window-resized` (`commands_frame._slot_sizes`).
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

    def test_a_component_with_no_number_to_echo_is_refused_rather_than_raising(self):
        """The third size policy, asked of the one component that really has it.

        `personas` is `Fill()` — neither a number to echo nor a content height to pin. It
        is a CHILD of the sidebar today, so it cannot reach `_built_in_size` through
        `builtins.SLOT_OF`, and that is exactly why the question is put to the function
        rather than to a committed file: `value == c.size.n` on a `Fill` is an
        `AttributeError`, raised while a committed file is being resolved — on the path of
        `charter --version` as much as `charter frame` — where this form's answer to a
        value it cannot honour is to refuse the arrangement and draw the `slots` frame.

        Without this the `isinstance(c.size, Fixed)` half of that condition is a survivor
        masked by the `Content` branch above it: every component `SLOT_OF` currently
        reaches is one or the other, so nothing could observe it. Asked here, it is the
        line that stands between a future `Fill` placement and a traceback.
        """
        fill = builtins.build().get("personas")
        self.assertIsInstance(fill.size, component.Fill)
        self.assertIsNone(instance._built_in_size(fill, 5))

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
        for content in (0, 1, 2, 14, 30):
            with self.subTest(content=content):
                self.assertEqual(
                    layout.repos_rows(content_rows=content, window_rows=50,
                                      slots=SLOTS, pinned_rows=_pin(size=15)),
                    15)

    def test_a_plane_that_pins_nothing_is_sized_by_its_content_exactly_as_before(self):
        """The default, asserted against a written-out arrangement rather than against a
        `slots` list — because a written-out arrangement is where a `repos` placement
        exists to be misread, and charter's own plane commits one. Two counts, so a pin
        read off the wrong placement (`identity` is `Fixed(1)` and comes first in file
        order) cannot pass by coincidence."""
        self.assertIsNone(_pin(), "a written-out arrangement with no size is not a pin")
        for content in (4, 7):
            with self.subTest(content=content):
                self.assertEqual(
                    layout.repos_rows(content_rows=content, window_rows=50,
                                      slots=SLOTS, pinned_rows=_pin()),
                    content)

    def test_the_pin_reaches_tmux_as_the_length_the_pane_is_split_to(self):
        """The seam that makes any of this visible: `slot_sizes` into `panel_argvs` into
        `-l`. Asserted as the literal string tmux would see, so a number computed
        correctly and then dropped on the way out is red."""
        sizes = layout.slot_sizes(SLOTS, window_rows=50, content_rows=2,
                                  pinned_rows=_pin(size=15))
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
        sizes = layout.slot_sizes(SLOTS, window_rows=rows, content_rows=2,
                                  pinned_rows=_pin(size=15))
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
        got = layout.repos_rows(content_rows=2, window_rows=rows, slots=SLOTS,
                                pinned_rows=_pin(size=15))
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
                pinned = layout.repos_rows(content_rows=0, window_rows=rows,
                                           slots=SLOTS, pinned_rows=_pin(size=99))
                grown = layout.repos_rows(content_rows=99, window_rows=rows,
                                          slots=SLOTS, pinned_rows=_pin())
                self.assertEqual(pinned, grown)

    def test_the_floor_holds_when_the_window_has_no_rows_to_spare_at_all(self):
        """Below the floor is a zero or negative `-l`, which tmux refuses outright — the
        frame would come up with no strip at all in exactly the terminal least able to
        afford a missing panel. What protects the harness there is `visible_slots`, which
        drops the slot rather than shrinking it."""
        self.assertEqual(
            layout.repos_rows(content_rows=0, window_rows=6, slots=SLOTS,
                              pinned_rows=_pin(size=15)),
            layout.SLOT_SIZE["repos"])


class TheCommittedFileIsReadAtABoundaryAndNotInTheArithmetic(unittest.TestCase):
    """#661: `layout` answers the same thing whatever this plane committed.

    **The defect this class exists for was not environmental.** Nine earlier instances of
    "the suite reads or spends the machine" read something that merely *differs* between
    machines — the clock, the cwd, `$COLUMNS`, a real vault, a real tmux. This one read
    `charter.toml`, a file tracked IN this repository, from inside `layout.repos_rows` —
    so the failure was deterministic and was triggered by following the documentation.
    Writing the three lines this feature's own news entry gives an operator turned six
    tests red, and committing them would have turned CI red for everyone.

    So the property is not "`repos_rows` happens to be right today". It is that no
    arrangement a plane can commit changes what these two functions answer, which is what
    `ReposIsSizedToItsContent`'s "pure arithmetic, so it is pinned here with no tmux and
    no cache" has always claimed and could not enforce.
    """

    def arrangements(self) -> dict:
        """Every shape a plane's `[[frame.component]]` tables can take on the repo table —
        no arrangement at all, one written out with no pin, and pins on either side of
        both bounds. If any of them reaches the arithmetic, one of the answers below is
        not the content the caller named."""
        return {"no arrangement": {},
                "written out, no size": _arrangement(),
                "pinned small": _arrangement(size=2),
                "pinned large": _arrangement(size=15),
                "pinned past the cap": _arrangement(size=99)}

    def test_no_arrangement_a_plane_can_commit_changes_what_repos_rows_answers(self):
        """The issue's own measurement, as a property: 4 rows of content in a 50-row
        window with neither bound binding can only be answered `4`. Asserted as the
        content and not as "the same for all five", because equal-and-wrong is exactly
        what a `repos_rows` that read the file would give five identical planes."""
        for name, frame in self.arrangements().items():
            with self.subTest(arrangement=name):
                with mock.patch.dict(config.FRAME, frame):
                    self.assertEqual(
                        layout.repos_rows(content_rows=4, window_rows=50,
                                          slots=["top", "bottom", "repos"]),
                        4)

    def test_no_arrangement_a_plane_can_commit_changes_what_slot_sizes_answers(self):
        """`slot_sizes` is the one callers actually reach, and three of #661's six red
        tests were its — so pinning only the leaf would have left the defect in the
        function above it. The whole map, so a `repos` answered from the file cannot hide
        behind the fixed strips being right."""
        for name, frame in self.arrangements().items():
            with self.subTest(arrangement=name):
                with mock.patch.dict(config.FRAME, frame):
                    self.assertEqual(
                        layout.slot_sizes(SLOTS, window_rows=50, content_rows=4),
                        {"top": 1, "bottom": 1, "repos": 4, "right": 22})

    def test_this_repositorys_own_committed_file_with_a_pin_in_it_is_not_read_here(self):
        """The reproduction from #661, run against the real file rather than a fixture.

        `charter.toml` is tracked here, and an operator adding `size = 15` to its `repos`
        table is doing what `docs/frame.md` tells them to do. The key is added to the
        table that is actually on disk rather than to a written-out copy of it — so a
        plane that later commits the same key for real is running the case this test
        already ran, and the day charter's own frame gains a pin this stays green rather
        than needing to be rewritten.
        """
        cfg = tomllib.loads(_COMMITTED.read_text(encoding="utf-8"))
        tables = cfg["frame"]["component"]
        repos, = [t for t in tables if t["use"] == "repos"]
        repos["size"] = 15
        resolved = instance.frame_of(cfg)
        self.assertEqual(
            [p["size"] for p in resolved["components"] if p["slot"] == "repos"],
            [component.Fixed(15)],
            "the pin did not survive the boundary, so nothing below could have read it "
            "even if the arithmetic still did")
        with mock.patch.dict(config.FRAME, resolved):
            self.assertEqual(
                layout.repos_rows(content_rows=4, window_rows=50,
                                  slots=["top", "bottom", "repos"]),
                4)
            self.assertEqual(
                layout.slot_sizes(resolved["slots"], window_rows=50, content_rows=6),
                {"top": 1, "bottom": 1, "repos": 6, "right": 22})

    def test_the_number_the_arithmetic_was_handed_is_the_number_it_used(self):
        """The other half, and it is what stops the two tests above being satisfied by a
        `repos_rows` that ignores pins altogether. Passed explicitly, with `config.FRAME`
        holding an arrangement that pins something ELSE — so an argument quietly dropped
        in favour of a re-read would answer 15 here, and a pin never read at all would
        answer 4."""
        with mock.patch.dict(config.FRAME, _arrangement(size=15)):
            self.assertEqual(
                layout.repos_rows(content_rows=4, window_rows=50,
                                  slots=["top", "bottom", "repos"], pinned_rows=7),
                7)


class TheBoundaryIsWhereThePlaneIsRead(unittest.TestCase):
    """`commands_frame._slot_sizes` — the one place a committed arrangement becomes a
    pane height, and the three callers that go through it.

    They differ only in how the table pane's width is arrived at (a launch derives it, a
    re-layout derives it from the panes that survived, a resize measures it), so the read
    is one line rather than three — which is also the answer to #660's "five signatures":
    the pin's path is `slot_sizes` and `repos_rows`, two, and this is the third place a
    line changed.
    """

    def test_the_boundary_reads_the_plane_and_hands_the_arithmetic_the_number(self):
        """`repos_rows_wanted` is stubbed to a number nothing else here could produce, so
        the assertion is about which of the two terms won rather than about whether the
        answer is plausible."""
        with mock.patch.dict(config.FRAME, _arrangement(size=15)), \
                mock.patch("charter.frame.slots.repos_rows_wanted", return_value=3):
            got = commands_frame._slot_sizes("f-1", SLOTS, window_rows=50, pane_cols=200)
        self.assertEqual(got["repos"], 15)

    def test_a_plane_that_pins_nothing_still_gets_its_clone_count_through(self):
        """The default path, through the same boundary. Without this, a `_slot_sizes` that
        passed `pinned_rows` and dropped `content_rows` would pass the case above."""
        with mock.patch.dict(config.FRAME, _arrangement()), \
                mock.patch("charter.frame.slots.repos_rows_wanted", return_value=3):
            got = commands_frame._slot_sizes("f-1", SLOTS, window_rows=50, pane_cols=200)
        self.assertEqual(got["repos"], 3)

    def test_the_resize_hook_sizes_the_strip_from_the_pin_and_not_the_clone_count(self):
        """`_reassert_sizes` runs on every `window-resized`, which is the path a frame
        spends its life on — and it is a SECOND call site, so a pin read only at launch
        would give the operator a strip that snapped back to its clone count the first
        time they dragged the divider.

        The strip is the pane tmux is never told the height of, so it is read back the way
        `test_frame_density` reads it: the window's rows, minus every height asserted,
        minus one border per horizontal split. `test_frame_tmux_integration` asks real
        tmux the same question; this asks charter's arithmetic, on a box with no tmux.
        """
        calls: list[list[str]] = []

        def fake(action, argv, *, env=None, timeout=None, report=True):
            calls.append(list(argv))
            return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")

        panes = {"top": "%1", "bottom": "%2", "repos": "%3"}
        with mock.patch.dict(config.FRAME, _arrangement(size=15)), \
                mock.patch("charter.frame.slots.repos_rows_wanted", return_value=3), \
                mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            commands_frame._reassert_sizes("sock", fid="f-1", panes=panes,
                                           harness_pane="%0", window_cols=200,
                                           window_rows=50)
        heights = {c[c.index("-t") + 1]: int(c[c.index("-y") + 1])
                   for c in calls if "resize-pane" in c and "-y" in c}
        self.assertNotIn(panes["repos"], heights,
                         "the strip is the dependent pane and must stay unasserted — "
                         "asserting N heights in an N-pane stack is what swapped the "
                         "table's and the attention strip's sizes on 3.7c")
        self.assertEqual(50 - sum(heights.values()) - len(heights), 15)
