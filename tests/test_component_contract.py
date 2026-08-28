"""What a component promises before anything places it or draws it.

The contract is the public seam a third-party provider binds to (spec §4b), and charter's
own components are its first consumers — so every rule here is enforced at construction,
where a provider meets it, rather than checked later by whichever call site happens to
remember.

**The id rules are a containment boundary, not a style preference.** A component id
reaches a menu row and a tmux pane title, and `instance._HOTKEY_RE` exists because a
newline in a committed `[frame] hotkey` reached tmux CONFIG TEXT and achieved code
execution at launch, silently, with no keypress. An id arrives from the same class of
place — a committed `charter.toml` names it, an installed provider declares it — so it is
held to the same closed alphabet, and the tests below spend most of their length on the
values that are refused rather than the one that is accepted.
"""

from __future__ import annotations

import dataclasses
import unittest

from charter.frame import component


def _c(**kw):
    """A valid component, with *kw* replacing one field at a time.

    Every case below differs from a working component in exactly one way, so a refusal
    can only be about the field the case names.
    """
    base = dict(id="demo", title="Demo", edge="right", size=component.Fixed(3),
                needs=("gather",), events=(), render=lambda ctx: ["x"])
    base.update(kw)
    return component.Component(**base)


class TheContract(unittest.TestCase):
    def test_a_component_declares_what_it_reads(self):
        c = component.Component(id="demo", title="Demo", edge="right",
                                size=component.Fixed(3), needs=("gather",),
                                events=(), render=lambda ctx: ["x"])
        self.assertEqual(c.needs, ("gather",))
        self.assertEqual(component.API_VERSION, 1)

    def test_a_component_cannot_be_edited_after_it_is_constructed(self):
        """Frozen, because every rule here is checked once, at construction.

        A mutable `edge` or `size` would move the whole of this file's enforcement to
        wherever the field is next read — which is the shape the registry is replacing,
        where a slot's list position was its geometry and nothing owned the rule.
        """
        c = _c()
        with self.assertRaises(dataclasses.FrozenInstanceError):
            c.edge = "top"

    def test_the_fields_are_keyword_only(self):
        """Field ORDER is not part of the contract, and must not become part of it.

        Task 3 adds `children`; a provider that had spelled its component positionally
        would break on that addition, having done nothing wrong.
        """
        with self.assertRaises(TypeError):
            component.Component("demo", "Demo", "right", component.Fixed(3),
                                (), (), lambda ctx: [])


class SizePolicies(unittest.TestCase):
    """Three policies, and each one validates its own number.

    Children declare, the parent arbitrates (spec §4e) — so a nonsense size is refused
    where it is written, not resolved as a tie-break while a frame is being laid out.
    """

    def test_the_three_policies_carry_what_the_spec_says_they_carry(self):
        self.assertEqual(component.Fixed(3).n, 3)
        self.assertIsNone(component.Content().cap)
        self.assertEqual(component.Content(9).cap, 9)
        self.assertEqual(component.Fill(), component.Fill())

    def test_a_fixed_size_must_be_a_positive_count_of_cells(self):
        for n in (0, -1, 1.5, "3", None):
            with self.subTest(n=n), self.assertRaises(component.ComponentError):
                component.Fixed(n)

    def test_a_bool_is_not_a_size(self):
        """`Fixed(True)` is `Fixed(1)` to `isinstance(n, int)`, and means nothing.

        The fixture-value class of defect this suite keeps finding: a value that passes
        the type check while carrying a different meaning entirely.
        """
        with self.assertRaises(component.ComponentError):
            component.Fixed(True)

    def test_a_content_cap_is_absent_or_a_positive_count(self):
        for cap in (0, -2, 1.5, "9", True):
            with self.subTest(cap=cap), self.assertRaises(component.ComponentError):
                component.Content(cap)

    def test_a_size_that_is_not_a_policy_is_refused(self):
        for size in (3, "content", None, component.Fixed):
            with self.subTest(size=size), self.assertRaises(component.ComponentError):
                _c(size=size)


class IdIsAContainmentBoundary(unittest.TestCase):
    def test_a_newline_in_an_id_is_refused(self):
        """`instance._HOTKEY_RE`'s incident, one surface over.

        The refusal itself is a line of charter's own output naming the offender, so the
        offender must not be able to write a second line of it either.
        """
        with self.assertRaises(component.ComponentError) as e:
            _c(id="demo\nbind -n F2 run-shell 'touch /tmp/PWNED'")
        self.assertNotIn("\n", str(e.exception))
        self.assertIn("PWNED", str(e.exception))       # named, not swallowed

    def test_an_id_ending_in_a_newline_is_refused(self):
        """Its own case, because it is the one a correct-looking pattern still admits.

        Python's `$` matches at the end of the string OR just before a trailing newline,
        so `_ID_RE.match("personas\\n")` succeeds against a pattern written to exclude
        newlines outright. `frame_of` reaches for `fullmatch` on `instance._HOTKEY_RE`
        for exactly this reason; the id in the middle of a line above is refused by a
        pattern with the hole still in it, and only this case fails when it is there.
        """
        with self.assertRaises(component.ComponentError):
            _c(id="personas\n")

    def test_a_namespaced_provider_id_is_accepted(self):
        self.assertEqual(_c(id="acme.metrics").id, "acme.metrics")

    def test_a_built_in_id_needs_no_namespace(self):
        for good in ("repos", "personas", "todos", "attention", "identity", "s1"):
            with self.subTest(id=good):
                self.assertEqual(_c(id=good).id, good)

    def test_one_dot_only(self):
        """Namespaced by distribution (§4h) — one segment, one id, and no more.

        A third level is not a deeper namespace; it is a second syntax nobody decided on.
        """
        with self.assertRaises(component.ComponentError):
            _c(id="acme.metrics.rows")

    def test_everything_outside_the_alphabet_is_refused(self):
        bad = ["", " ", "Repos", "re pos", "repos;ls", "../repos", "repos/x",
               "repos\x1b[31m", "acme.", ".metrics", "acme..metrics", "1repos",
               "repos\r", "repos\t", "repos$(id)", "repos'", 'repos"', "repos#{x}",
               "repos{}", "repos-x", "repos​", "a" * 65, "_repos", None, 7, b"repos"]
        for value in bad:
            with self.subTest(id=value), self.assertRaises(component.ComponentError):
                _c(id=value)

    def test_the_refusal_says_what_a_usable_id_looks_like(self):
        """A refusal a provider author cannot act on costs them a bisect."""
        with self.assertRaises(component.ComponentError) as e:
            _c(id="Repos")
        self.assertIn("Repos", str(e.exception))
        self.assertIn(component.ID_HINT, str(e.exception))


class TitleCannotForgeARow(unittest.TestCase):
    def test_a_newline_in_a_title_is_contained_rather_than_refused(self):
        """A title is display text with an open alphabet; an id is a name with a closed
        one. So the title is contained the way every other committed display value is,
        and what a reader would recognise survives.
        """
        c = _c(title="Demo\n  fake  fake")
        self.assertNotIn("\n", c.title)
        self.assertIn("\\x0a", c.title)
        self.assertIn("Demo", c.title)

    def test_a_title_that_is_not_text_is_refused(self):
        for title in (None, 7, ["Demo"]):
            with self.subTest(title=title), self.assertRaises(component.ComponentError):
                _c(title=title)


class EdgeIsAClosedSet(unittest.TestCase):
    def test_the_four_edges_are_accepted(self):
        for edge in component.EDGES:
            with self.subTest(edge=edge):
                self.assertEqual(_c(edge=edge).edge, edge)

    def test_anything_else_is_refused_naming_the_edges(self):
        for edge in ("middle", "Right", "", None, "right ", "left,right"):
            with self.subTest(edge=edge), self.assertRaises(component.ComponentError) as e:
                _c(edge=edge)
            self.assertIn("bottom", str(e.exception))


class NeedsAreTheDeclaredCost(unittest.TestCase):
    def test_a_slice_nobody_serves_is_refused_at_construction(self):
        """A typo in `needs` is a component that silently reads nothing.

        `ctx` is built FROM `needs` (§4e), so an unserved name would produce an
        attribute that is simply absent at render time — a blank pane, which is the
        confidently-wrong output the left sidebar was retired for.
        """
        with self.assertRaises(component.ComponentError) as e:
            _c(needs=("gathr",))
        self.assertIn("gathr", str(e.exception))
        self.assertIn("gather", str(e.exception))      # names what is on offer

    def test_a_bare_string_is_not_a_tuple_of_needs(self):
        """`needs="gather"` is a sequence of six one-letter needs to anything that
        iterates it, and every one of them would be refused for the wrong reason."""
        with self.assertRaises(component.ComponentError) as e:
            _c(needs="gather")
        self.assertIn("tuple", str(e.exception))

    def test_a_list_of_needs_becomes_a_tuple(self):
        self.assertEqual(_c(needs=["gather", "todos"]).needs, ("gather", "todos"))

    def test_declaring_nothing_is_allowed_and_is_the_cheap_case(self):
        self.assertEqual(_c(needs=()).needs, ())


class EventsAreTheClosedFive(unittest.TestCase):
    def test_every_named_kind_is_accepted(self):
        """The `on_event` is not decoration here: since #607 a declaration with nothing
        behind it is refused, so "every kind is accepted" can only be asked of a
        component that could actually receive one."""
        self.assertEqual(_c(events=component.EVENT_KINDS,
                            on_event=lambda ev: None).events,
                         tuple(component.EVENT_KINDS))

    def test_drag_was_deliberately_excluded_and_stays_excluded(self):
        """§4f: adding `drag` later costs nothing; removing it after a provider has
        shipped against it costs everything."""
        with self.assertRaises(component.ComponentError) as e:
            _c(events=("drag",))
        self.assertIn("drag", str(e.exception))
        self.assertIn("scroll", str(e.exception))

    def test_a_bare_string_is_not_a_tuple_of_events(self):
        with self.assertRaises(component.ComponentError):
            _c(events="key")


class ChildrenAreIdsOfTheSameShape(unittest.TestCase):
    """What a composite's parts LOOK like is this module's rule; what they refer to is
    the registry's, because only the registry sees every component at once."""

    def test_a_leaf_declares_no_children(self):
        self.assertEqual(_c().children, ())

    def test_children_are_kept_in_declared_order(self):
        self.assertEqual(_c(children=["personas", "todos"]).children,
                         ("personas", "todos"))

    def test_a_child_id_is_held_to_the_same_alphabet_as_an_id(self):
        for child in ("Personas", "personas\n", "../personas", ""):
            with self.subTest(child=child), self.assertRaises(component.ComponentError):
                _c(children=(child,))

    def test_a_bare_string_is_not_a_tuple_of_children(self):
        with self.assertRaises(component.ComponentError):
            _c(children="personas")


class RenderIsCallable(unittest.TestCase):
    def test_a_renderer_that_cannot_be_called_is_refused(self):
        for r in (None, "render", ["x"]):
            with self.subTest(render=r), self.assertRaises(component.ComponentError):
                _c(render=r)

    def test_the_renderer_is_kept_as_handed_over(self):
        def rendered(ctx):
            return ["x"]
        self.assertIs(_c(render=rendered).render, rendered)


if __name__ == "__main__":                                 # pragma: no cover
    unittest.main()
