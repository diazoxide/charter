"""Three ways of asking for a frame resolve to one arrangement, and none of them moves.

`slots` is shorthand for placing built-ins on their own edges in the given split order;
`density` is a named arrangement; `[[frame.component]]` is the arrangement written out.
`instance.frame_components` is where all three become one list of placements, and these
pin that the two older spellings resolve to exactly the frame they drew before.

**The repo's own committed `charter.toml` is tested by name, not by fixture.** A change
that silently removed the repo table from charter's own plane has already shipped once
(#535) and was caught by a reviewer rather than by a test. `TheOperatorsOwnPlane` below is
that test: it reads the file this repository commits, resolves it, and asserts the table
is there and at the width its own comment promises.
"""

from __future__ import annotations

import tomllib
import unittest
from pathlib import Path

from charter import instance
from charter.frame import builtins, layout

#: The repository's own committed `charter.toml` — the plane charter is developed on.
#: `parents[1]` is the checkout root, the idiom `tests/test_docs_show.py` already uses.
REPO_ROOT = Path(__file__).resolve().parents[1]
CHARTER_TOML = REPO_ROOT / "charter.toml"


def _tables(placements) -> list[dict]:
    """*placements* written back out as `[[frame.component]]` tables would spell them."""
    out = []
    for p in placements:
        table = {"use": p["use"], "edge": p["edge"]}
        if hasattr(p["size"], "n"):
            table["size"] = p["size"].n
        if not p["visible"]:
            table["visible"] = False
        out.append(table)
    return out


class SlotsIsShorthandForPlacingBuiltIns(unittest.TestCase):
    def test_the_shipped_default_places_every_panel_charter_draws(self):
        """Literals, in split order. Reading the expectation back out of
        `instance.FRAME_DEFAULTS` would pass against a default that had lost a panel."""
        got = instance.frame_components({})
        self.assertEqual([(p["use"], p["edge"], p["visible"]) for p in got],
                         [("identity", "top", True), ("attention", "bottom", True),
                          ("repos", "bottom", True), ("sidebar", "right", True)])

    def test_an_operators_slot_order_is_the_split_order_of_the_arrangement(self):
        """Order is geometry, so it survives the mapping unchanged. `["right", "top"]` is
        the shortest list that is not already in the shipped list's relative order —
        `tests/test_frame_config.py` uses it for the same reason."""
        got = instance.frame_components({"frame": {"slots": ["right", "top"]}})
        self.assertEqual([p["use"] for p in got], ["sidebar", "identity"])

    def test_a_retired_slot_name_is_dropped_before_it_becomes_a_placement(self):
        """`left` was retired in #488 and a committed charter.toml outlives an upgrade.
        The filtering is `frame_of`'s, done once, and the arrangement inherits it rather
        than reading the committed list a second time."""
        got = instance.frame_components(
            {"frame": {"slots": ["top", "bottom", "left", "right"]}})
        self.assertEqual([p["use"] for p in got],
                         ["identity", "attention", "sidebar"])

    def test_every_placement_carries_the_edge_and_size_its_component_declares(self):
        """The arrangement is the registry's answer, not a second table of geometry."""
        reg = builtins.build()
        for p in instance.frame_components({}):
            with self.subTest(component=p["use"]):
                c = reg.get(p["use"])
                self.assertEqual((p["edge"], p["size"]), (c.edge, c.size))


class DensityIsANamedArrangement(unittest.TestCase):
    def test_each_shipped_level_resolves_to_the_panels_it_names(self):
        """Asserted as literals per level, because the point of a preset is that it
        expands to a frame an operator could have written by hand — and that expansion is
        what a reader has to be able to check."""
        want = {
            "minimal": ["identity", "attention"],
            "normal": ["identity", "attention", "repos"],
            "full": ["identity", "attention", "repos", "sidebar"],
        }
        self.assertEqual(sorted(want), sorted(instance.FRAME_DENSITY))
        for level, expect in want.items():
            with self.subTest(density=level):
                got = instance.frame_components({"frame": {"density": level}})
                self.assertEqual([p["use"] for p in got], expect)

    def test_an_explicit_slots_list_still_beats_a_declared_density(self):
        """`slots` is the primitive and an operator who wrote a list meant that list —
        `frame_of`'s rule, inherited rather than re-decided here."""
        got = instance.frame_components(
            {"frame": {"density": "minimal", "slots": ["top", "repos"]}})
        self.assertEqual([p["use"] for p in got], ["identity", "repos"])


class TheArrangementWrittenOut(unittest.TestCase):
    def test_file_order_is_split_order(self):
        cfg = {"frame": {"component": [{"use": "sidebar"}, {"use": "identity"}]}}
        self.assertEqual(instance.frame_of(cfg)["slots"], ["right", "top"])
        self.assertEqual([p["use"] for p in instance.frame_components(cfg)],
                         ["sidebar", "identity"])

    def test_a_component_can_be_kept_in_the_arrangement_and_not_drawn(self):
        """The one thing `slots` cannot express: deleting a name loses its POSITION with
        it, so turning a panel back on means remembering where in the order it went."""
        cfg = {"frame": {"component": [{"use": "identity"}, {"use": "attention"},
                                       {"use": "repos", "visible": False},
                                       {"use": "sidebar"}]}}
        self.assertEqual(instance.frame_of(cfg)["slots"], ["top", "bottom", "right"])
        got = instance.frame_components(cfg)
        self.assertEqual([(p["use"], p["visible"]) for p in got],
                         [("identity", True), ("attention", True),
                          ("repos", False), ("sidebar", True)])

    def test_an_arrangement_beats_both_slots_and_density(self):
        cfg = {"frame": {"density": "minimal", "slots": ["top", "bottom", "repos"],
                         "component": [{"use": "sidebar"}]}}
        self.assertEqual(instance.frame_of(cfg)["slots"], ["right"])

    def test_every_slots_list_round_trips_through_the_written_out_form(self):
        """**Lossless in both directions**, which is what makes `slots` retirable later
        without an operator's committed frame changing under them: resolve a `slots` list
        to placements, write those placements out as tables, resolve the tables, and get
        the same placements and the same split order back."""
        for slots in (["top", "bottom", "repos", "right"], ["right", "top"],
                      ["top", "bottom"], ["repos"]):
            with self.subTest(slots=slots):
                first = instance.frame_components({"frame": {"slots": slots}})
                cfg = {"frame": {"component": _tables(first)}}
                self.assertEqual(instance.frame_components(cfg), first)
                self.assertEqual(instance.frame_of(cfg)["slots"], slots)


class AnArrangementCharterCannotDrawIsRefusedWhole(unittest.TestCase):
    """#535 is why this is refused whole rather than one table at a time.

    Dropping just the table charter cannot make sense of hands the operator a frame with
    a panel silently missing — and a missing repo table is a plane that appears to have no
    clones. Every case below asserts the same thing: the frame falls back to the one
    `slots` describes, and the repo table is still in it.
    """

    def _fallback(self, tables, *, slots=None):
        frame = {"component": tables}
        if slots is not None:
            frame["slots"] = slots
        got = instance.frame_of({"frame": frame})["slots"]
        self.assertEqual(got, slots or list(instance.FRAME_DEFAULTS["slots"]))
        self.assertIn("repos", got)

    def test_a_component_charter_has_never_heard_of(self):
        self._fallback([{"use": "identity"}, {"use": "acme.build"}])

    def test_the_old_slot_vocabulary_written_into_use(self):
        """`use` names a component, not a slot. `top` is a slot name, so a file mixing
        the two is refused rather than half-understood — `builtins.SLOT_OF` is the one
        table between the vocabularies and this keeps it the only one."""
        self._fallback([{"use": "top"}, {"use": "bottom"}])

    def test_an_edge_charter_cannot_place_the_component_at(self):
        """Charter derives the whole frame's geometry from the components' own
        declarations and nothing between here and `split-window` carries a per-plane
        override yet. So an edge charter would not honour refuses the arrangement rather
        than being read, validated, stored and ignored."""
        self._fallback([{"use": "repos", "edge": "top"}], slots=["top", "repos"])

    def test_a_size_charter_cannot_give_the_component(self):
        self._fallback([{"use": "sidebar", "size": 30}])

    def test_a_size_on_a_component_whose_height_is_its_content(self):
        """`repos` is `Content()`: its height is the plane's repo count bounded by what
        the harness may not be charged, so there is no number to accept here at all."""
        self._fallback([{"use": "repos", "size": 4}])

    def test_the_same_component_claimed_twice(self):
        """One pane draws it, and two claims have no answer — the registry's own rule
        (`Registry.register`), kept at the config boundary too."""
        self._fallback([{"use": "repos"}, {"use": "repos"}])

    def test_a_key_the_form_does_not_have(self):
        """A typo in a key name is a line that does nothing, which is the same class of
        silence as a value that does nothing."""
        self._fallback([{"use": "identity", "edges": "top"}])

    def test_a_visible_that_is_not_a_yes_or_no(self):
        self._fallback([{"use": "identity", "visible": "no"}])

    def test_an_arrangement_that_is_not_an_array_of_tables(self):
        for junk in ("identity", {"use": "identity"}, [], ["identity"], 3):
            with self.subTest(junk=junk):
                got = instance.frame_of({"frame": {"component": junk}})["slots"]
                self.assertEqual(got, list(instance.FRAME_DEFAULTS["slots"]))

    def test_the_component_keys_a_declared_arrangement_may_carry(self):
        """Named as a closed set, so a key added to the form has to be added here too."""
        self.assertEqual(instance.FRAME_COMPONENT_FIELDS,
                         ("use", "edge", "size", "visible"))


class OrderIsGeometry(unittest.TestCase):
    """The measurement `slots`' order exists to protect, asked of the resolved frame.

    Measured on tmux 3.7c in a 200x50 window: `["top", "bottom", "right"]` gives a
    **200-column** bottom row, and `["top", "right", "bottom"]` gives **177** — inset
    beside the one sidebar and its border. (The spec's §4/§4b carry 154, which is the
    pre-#488 arrangement with two 22-column sidebars coming off the row; §4i corrects it.)
    """

    def test_the_table_gets_the_whole_window_when_the_sidebar_is_split_after_it(self):
        slots = instance.frame_of(
            {"frame": {"slots": ["top", "bottom", "repos", "right"]}})["slots"]
        self.assertEqual(layout.repos_cols(slots, window_cols=200), 200)

    def test_the_table_is_inset_beside_a_sidebar_split_before_it(self):
        slots = instance.frame_of(
            {"frame": {"slots": ["top", "right", "bottom", "repos"]}})["slots"]
        self.assertEqual(layout.repos_cols(slots, window_cols=200), 177)
        self.assertEqual(177, 200 - layout.SLOT_SIZE["right"] - layout._BORDER_COLS)

    def test_the_written_out_arrangement_keeps_the_same_two_geometries(self):
        """The mapping cannot be lossy about the one thing the order decides."""
        for tables, want in (([{"use": "identity"}, {"use": "attention"},
                               {"use": "repos"}, {"use": "sidebar"}], 200),
                             ([{"use": "identity"}, {"use": "sidebar"},
                               {"use": "attention"}, {"use": "repos"}], 177)):
            with self.subTest(want=want):
                slots = instance.frame_of({"frame": {"component": tables}})["slots"]
                self.assertEqual(layout.repos_cols(slots, window_cols=200), want)


class TheOperatorsOwnPlane(unittest.TestCase):
    """This repository's committed `charter.toml`, resolved — #535's test.

    Reads the file the repo commits rather than a fixture of it, because a fixture is a
    copy that agrees with the real thing until somebody edits one of them. That is the
    whole failure mode: the change #535 shipped removed the repo table from charter's own
    plane and every fixture in the suite went on describing a frame that had one.
    """

    def setUp(self) -> None:
        self.assertTrue(CHARTER_TOML.is_file(),
                        f"{CHARTER_TOML} is missing — this repo IS a control plane")
        self.cfg = tomllib.loads(CHARTER_TOML.read_text())

    def test_charters_own_plane_still_draws_every_panel_it_draws_today(self):
        got = instance.frame_components(self.cfg)
        self.assertEqual([(p["use"], p["edge"], p["visible"]) for p in got],
                         [("identity", "top", True), ("attention", "bottom", True),
                          ("repos", "bottom", True), ("sidebar", "right", True)])

    def test_charters_own_plane_still_has_its_repo_table(self):
        """Said separately from the list above, and on purpose. The assertion that shape
        changed is one a future edit updates by pasting in whatever it now returns; this
        one names the panel and cannot be satisfied that way."""
        drawn = [p["use"] for p in instance.frame_components(self.cfg) if p["visible"]]
        self.assertIn("repos", drawn)

    def test_the_table_is_split_before_the_sidebar_so_it_gets_the_full_width(self):
        """charter.toml's own comment promises this: `repos` is split before `right`, so
        its table gets the full window width and is drawn at 95 columns and up. Listed
        after the sidebar it would need a 118-column terminal before the table was drawn
        at all, and below that the slot is dropped and the pane's rows go to the harness
        (#500)."""
        slots = instance.frame_of(self.cfg)["slots"]
        self.assertLess(slots.index("repos"), slots.index("right"))
        self.assertEqual(layout.repos_cols(slots, window_cols=200), 200)

    def test_the_committed_slot_list_is_the_one_this_charter_can_place(self):
        """Every name in the committed file resolves to a component. A plane whose
        charter.toml names something this charter retired would draw fewer panels than
        its file asks for, which is the upgrade case `instance.FRAME_SLOTS` filters —
        asserted here against the file that is actually committed."""
        for slot in self.cfg["frame"]["slots"]:
            with self.subTest(slot=slot):
                self.assertIn(slot, builtins.COMPONENT_OF)


if __name__ == "__main__":
    unittest.main()
