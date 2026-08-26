"""What is on screen, in what order, and which pane owns which component.

Two properties carry the whole file.

**Order is geometry.** `layout.panel_argvs` splits each panel off the harness pane in
order, so what a component is placed AFTER decides how much room it gets — measured
against tmux 3.7c in a 200x50 window (#386). The registry therefore stores the order it
was given, explicitly, and the tests below assert against an order that is neither
alphabetical nor grouped by edge, so nothing can pass by deriving it from a list position
or a sorted dict.

**Composition is one level and arbitrates exactly one `Fill()`** (§4e, §4h). Both are
registration-time errors naming both offenders, because a tie broken at layout time
produces a frame that shifts with the data — which reads as a bug every time, and cannot
be reproduced from the config that caused it.
"""

from __future__ import annotations

import unittest

from charter.frame import component, registry


def _c(cid, **kw):
    """A leaf component with a valid everything, differing only where a case says so."""
    base = dict(id=cid, title=cid.title(), edge="right", size=component.Fixed(3),
                needs=(), events=(), render=lambda ctx: [cid])
    base.update(kw)
    return component.Component(**base)


class RegistrationOrderIsSplitOrder(unittest.TestCase):
    def setUp(self):
        self.r = registry.Registry()

    def test_two_components_on_an_edge_come_back_in_registration_order(self):
        self.r.register(_c("zulu", edge="bottom"))
        self.r.register(_c("alpha", edge="bottom"))
        self.assertEqual([c.id for c in self.r.on_edge("bottom")], ["zulu", "alpha"])

    def test_order_is_neither_alphabetical_nor_grouped_by_edge(self):
        """The two orders a careless implementation would produce instead.

        `["top", "bottom", "right"]` gives a 200-column bottom row and
        `["top", "right", "bottom"]` gives 177 — the same three components, the same
        edges, a different geometry — so an `all()` that sorted or grouped would be
        returning a different frame while looking correct.
        """
        for cid, edge in (("top_bar", "top"), ("side", "right"), ("attention", "bottom")):
            self.r.register(_c(cid, edge=edge))
        self.assertEqual([c.id for c in self.r.all()], ["top_bar", "side", "attention"])

    def test_the_order_is_stored_as_a_number_rather_than_a_list_position(self):
        """Asked of the registry, not read off `all()`.

        A caller that needs "which split is this" — `layout`, and every future
        re-application of sizes — must be able to ask, rather than infer it from where
        the component happened to land in a list somebody may later filter.
        """
        self.r.register(_c("first", edge="top"))
        self.r.register(_c("second", edge="bottom"))
        self.assertLess(self.r.split_order("first"), self.r.split_order("second"))

    def test_a_filtered_view_does_not_renumber_what_is_left(self):
        """`on_edge` is a filter over one order, not an order of its own."""
        for cid, edge in (("a_one", "top"), ("b_two", "right"), ("c_three", "right")):
            self.r.register(_c(cid, edge=edge))
        self.assertEqual([self.r.split_order(c.id) for c in self.r.on_edge("right")],
                         [self.r.split_order("b_two"), self.r.split_order("c_three")])
        self.assertNotEqual(self.r.split_order("b_two"), 0)


class OneIdOneComponent(unittest.TestCase):
    def setUp(self):
        self.r = registry.Registry()

    def test_a_duplicate_id_is_refused_naming_both(self):
        """§4h: silently picking one means the frame shows something whose origin cannot
        be determined, and "which of my two plugins drew this" is a debugging problem
        with no entry point."""
        self.r.register(_c("metrics", title="Ours"))
        with self.assertRaises(component.ComponentError) as e:
            self.r.register(_c("metrics", title="Theirs"))
        self.assertIn("metrics", str(e.exception))
        self.assertIn("Ours", str(e.exception))
        self.assertIn("Theirs", str(e.exception))

    def test_the_first_registration_survives_the_refusal(self):
        """A refused second registration must not have half-replaced the first."""
        self.r.register(_c("metrics", title="Ours", edge="top"))
        with self.assertRaises(component.ComponentError):
            self.r.register(_c("metrics", title="Theirs", edge="bottom"))
        self.assertEqual(self.r.get("metrics").title, "Ours")
        self.assertEqual([c.id for c in self.r.on_edge("top")], ["metrics"])
        self.assertEqual(self.r.on_edge("bottom"), ())

    def test_asking_for_a_component_nobody_registered_names_it(self):
        with self.assertRaises(component.ComponentError) as e:
            self.r.get("acme.metrics")
        self.assertIn("acme.metrics", str(e.exception))

    def test_an_unknown_id_cannot_forge_the_refusal_that_names_it(self):
        with self.assertRaises(component.ComponentError) as e:
            self.r.get("acme\nmetrics")
        self.assertNotIn("\n", str(e.exception))

    def test_registering_something_that_is_not_a_component_is_refused(self):
        for thing in (None, "repos", {"id": "repos"}):
            with self.subTest(thing=thing), self.assertRaises(component.ComponentError):
                self.r.register(thing)


class EdgesAreAsked(unittest.TestCase):
    def setUp(self):
        self.r = registry.Registry()

    def test_an_edge_with_nothing_on_it_is_empty(self):
        self.assertEqual(self.r.on_edge("left"), ())

    def test_a_misspelled_edge_is_a_refusal_rather_than_an_empty_frame(self):
        """`on_edge("botom")` answering `()` is a silently missing panel."""
        with self.assertRaises(component.ComponentError) as e:
            self.r.on_edge("botom")
        self.assertIn("botom", str(e.exception))


class CompositionIsOneLevel(unittest.TestCase):
    """A component owns a pane; charter never draws splits inside one (§4d).

    A composite is how N things share a pane, and its children are placed by it rather
    than by the frame — so they leave `on_edge`, which is what stops the same renderer
    being drawn twice, once in its own pane and once inside its parent's.
    """

    def setUp(self):
        self.r = registry.Registry()
        self.r.register(_c("personas", size=component.Content(cap=8)))
        self.r.register(_c("todos", size=component.Fill()))

    def _sidebar(self, **kw):
        base = dict(id="sidebar", children=("personas", "todos"))
        base.update(kw)
        return _c(base.pop("id"), **base)

    def test_a_composite_holds_its_children_in_declared_order(self):
        self.r.register(self._sidebar())
        self.assertEqual([c.id for c in self.r.children_of("sidebar")],
                         ["personas", "todos"])

    def test_a_child_is_no_longer_placed_on_its_own_edge(self):
        self.r.register(self._sidebar())
        self.assertEqual([c.id for c in self.r.on_edge("right")], ["sidebar"])

    def test_the_children_are_still_registered_and_reachable(self):
        """They are hidden from placement, not deleted — `get` still answers, so a menu
        row and a future per-child focus have something to name."""
        self.r.register(self._sidebar())
        self.assertEqual(self.r.get("personas").id, "personas")
        self.assertIn("personas", [c.id for c in self.r.all()])

    def test_exactly_one_child_fills_what_is_left(self):
        self.r.register(_c("alerts", size=component.Fill()))
        with self.assertRaises(component.ComponentError) as e:
            self.r.register(self._sidebar(children=("personas", "todos", "alerts")))
        self.assertIn("todos", str(e.exception))
        self.assertIn("alerts", str(e.exception))
        self.assertIn("sidebar", str(e.exception))

    def test_a_composite_with_no_filling_child_is_refused_too(self):
        """Zero is as unarbitrated as two: nothing has been told what the leftover rows
        of the pane belong to, and the answer would come from whichever renderer happened
        to run out of content first."""
        self.r.register(_c("counts", size=component.Fixed(2)))
        with self.assertRaises(component.ComponentError) as e:
            self.r.register(self._sidebar(children=("personas", "counts")))
        self.assertIn("sidebar", str(e.exception))
        self.assertIn("Fill", str(e.exception))

    def test_a_composite_of_a_composite_is_refused_naming_both(self):
        """One level (§4h). Arbitrary nesting is the layout engine §4d refused, wearing
        a different hat."""
        self.r.register(self._sidebar())
        self.r.register(_c("filler", size=component.Fill()))
        with self.assertRaises(component.ComponentError) as e:
            self.r.register(_c("outer", children=("sidebar", "filler")))
        self.assertIn("outer", str(e.exception))
        self.assertIn("sidebar", str(e.exception))

    def test_a_child_nobody_registered_is_refused_naming_it(self):
        with self.assertRaises(component.ComponentError) as e:
            self.r.register(self._sidebar(children=("personas", "todoz")))
        self.assertIn("todoz", str(e.exception))

    def test_a_composite_cannot_contain_itself(self):
        with self.assertRaises(component.ComponentError):
            self.r.register(_c("sidebar", children=("sidebar",)))

    def test_a_child_cannot_belong_to_two_composites(self):
        """Two panes claiming one renderer is a placement with two answers."""
        self.r.register(self._sidebar())
        with self.assertRaises(component.ComponentError) as e:
            self.r.register(_c("other", children=("todos", "personas")))
        self.assertIn("sidebar", str(e.exception))
        self.assertIn("other", str(e.exception))

    def test_a_refused_composite_leaves_its_children_placed_as_they_were(self):
        with self.assertRaises(component.ComponentError):
            self.r.register(self._sidebar(children=("personas", "todoz")))
        self.assertEqual([c.id for c in self.r.on_edge("right")], ["personas", "todos"])
        with self.assertRaises(component.ComponentError):
            self.r.get("sidebar")

    def test_children_of_a_leaf_is_empty_rather_than_an_error(self):
        self.assertEqual(self.r.children_of("personas"), ())


class Registries(unittest.TestCase):
    def test_two_registries_do_not_share_state(self):
        """Every test above builds its own, and a module-level dict shared behind them
        would make that isolation a fiction — the shape `PersonaIso` exists for, one
        layer down."""
        a, b = registry.Registry(), registry.Registry()
        a.register(_c("repos", edge="bottom"))
        self.assertEqual(b.all(), ())
        self.assertEqual([c.id for c in a.all()], ["repos"])


if __name__ == "__main__":                                 # pragma: no cover
    unittest.main()
