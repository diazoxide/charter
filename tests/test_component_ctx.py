"""A component receives what it declared, and there is nothing else on the object.

This is the decision the whole extension model rests on (§4e): `ctx` is constructed FROM
`needs`, so the idle-cost property — one `stat` per panel per tick — is enforced by what
was handed over rather than by a reviewer counting `stat`s in code charter did not write.

**Absent, not disabled.** A component that did not declare a slice does not get an
attribute holding `None`, an empty stand-in, or a handle that raises when used: the
attribute is not there, and the `AttributeError` names what to add. A present-but-empty
attribute is indistinguishable from a slice that happened to be empty, which is how a
component ships reading nothing and nobody notices until an operator's frame is blank.

The attribute set is asserted exactly, twice over, so a field cannot be added to `ctx`
without a test change — which is the point: every future field is a widening of what a
stranger's code may reach, and it should cost a conversation.
"""

from __future__ import annotations

import types
import unittest

from charter.frame import component, ctx


def _snapshot() -> dict:
    """A plane scan of the shape `gather.scan` produces — fresh each call.

    Fresh because a shared fixture that any test could edit is a fixture the next test
    reads instead of its own.
    """
    return {"gathered_at": 1755000000.0, "workspace": "plane", "current_repo": "charter",
            "repos": [{"name": "charter", "dirty": 0}, {"name": "other", "dirty": 2}],
            "worktrees": [], "todos": [{"title": "one"}, {"title": "two"}],
            "todo_count": 2}


#: "the caller said nothing" as a value of its own, because `None` is one of the
#: snapshots a test below hands over ON PURPOSE and must reach `build` unchanged.
_UNSAID = object()


def _ctx_for(needs=(), *, width=80, height=10, fid="f1", snapshot=_UNSAID):
    return ctx.build(needs, width=width, height=height, fid=fid,
                     snapshot=_snapshot() if snapshot is _UNSAID else snapshot)


class OnlyWhatWasDeclared(unittest.TestCase):
    def test_a_component_gets_only_what_it_declared(self):
        c = _ctx_for(needs=("gather",), width=80, height=10)
        self.assertEqual(c.width, 80)
        self.assertIsNotNone(c.gather)
        with self.assertRaises(AttributeError):
            c.todos             # not declared -> not present

    def test_the_attribute_set_is_exactly_the_declaration_plus_the_geometry(self):
        """Both halves of "exactly": the instance's own fields, and everything a
        component could reach by name on the object."""
        c = _ctx_for(needs=("repos", "todos"))
        expected = {"width", "height", "fid", "repos", "todos"}
        self.assertEqual(set(vars(c)), expected)
        self.assertEqual({n for n in dir(c) if not n.startswith("_")}, expected)

    def test_declaring_nothing_leaves_the_geometry_and_nothing_else(self):
        c = _ctx_for(needs=())
        self.assertEqual(set(vars(c)), {"width", "height", "fid"})

    def test_the_refusal_names_the_slice_and_where_to_declare_it(self):
        c = _ctx_for(needs=("gather",))
        with self.assertRaises(AttributeError) as e:
            c.repos
        self.assertIn("repos", str(e.exception))
        self.assertIn("needs", str(e.exception))

    def test_an_attribute_nobody_serves_at_all_is_refused_the_same_way(self):
        """A typo reads like a slice that is turned off, and it must not."""
        c = _ctx_for(needs=("gather",))
        with self.assertRaises(AttributeError) as e:
            c.reposs
        self.assertIn("reposs", str(e.exception))

    def test_a_slice_that_is_not_served_cannot_be_asked_for(self):
        with self.assertRaises(component.ComponentError) as e:
            _ctx_for(needs=("filesystem",))
        self.assertIn("filesystem", str(e.exception))

    def test_two_components_reading_one_snapshot_get_different_objects(self):
        """The property the mutation "hand over the whole snapshot" breaks."""
        snap = _snapshot()
        wide = ctx.build(("gather", "repos"), width=80, height=10, fid="f1",
                         snapshot=snap)
        narrow = ctx.build(("todos",), width=80, height=10, fid="f1", snapshot=snap)
        self.assertEqual(set(vars(narrow)), {"width", "height", "fid", "todos"})
        self.assertIn("gather", vars(wide))


class NoEscapeHatches(unittest.TestCase):
    """`ctx` hands over data. It hands over no way to *do* anything.

    Not a sandbox, and this file does not pretend it is one: a determined provider can
    import the standard library like any other Python code. What this rules out is the
    accident and the shortcut — a handle sitting on `ctx` that a renderer reaches for
    because it was there, whose cost then lands on every operator's tick.
    """

    def test_nothing_on_a_ctx_can_be_called(self):
        c = _ctx_for(needs=tuple(component.NEEDS))
        for name, value in vars(c).items():
            with self.subTest(attr=name):
                self.assertFalse(callable(value))
                self.assertNotIsInstance(value, types.ModuleType)

    def test_nothing_on_a_ctx_is_a_file_a_socket_or_a_path(self):
        c = _ctx_for(needs=tuple(component.NEEDS))
        for name, value in vars(c).items():
            with self.subTest(attr=name):
                for hatch in ("fileno", "read", "write", "connect", "open", "joinpath",
                              "__enter__", "__fspath__"):
                    self.assertFalse(hasattr(value, hatch), f"{name}.{hatch}")

    def test_a_component_cannot_bolt_a_field_onto_its_own_ctx(self):
        """One repaint's `ctx` is not a place to keep state, and a composite's parts
        must not be able to leave each other messages on it."""
        c = _ctx_for(needs=("gather",))
        with self.assertRaises(AttributeError):
            c.subprocess = "sh"
        with self.assertRaises(AttributeError):
            del c.width

    def test_the_served_slices_are_the_declared_ones_and_no_others(self):
        """One vocabulary: what `needs` may name and what `build` can serve are the same
        list, asked of each other rather than spelled twice.

        Two lists for one concept is the defect `contain.py` and `test_claims.py` were
        both rewritten for — one of the spellings always ends up missing something.
        """
        self.assertEqual(set(ctx.SERVES), set(component.NEEDS))


class TheVocabularyIsLookedUpBesideTheClass(unittest.TestCase):
    """Which contract a ctx answers over is registered beside its class, never ON it.

    The first cut of the second contract (`frame.action.ActionCtx`) put the table on the
    ctx class as four underscore-prefixed attributes. `type(ctx)._serves["vault"]` is a
    callable that reads this plane's vault registry and ignores the snapshot handed to
    it, so an action that declared NOTHING reached the whole inventory straight off its
    own ctx's class — and every assertion about the attribute set above missed it,
    because all of them filter names starting with ``_``. `frame.ctx.declare` and
    :data:`frame.ctx._CONTRACTS` are the fix; `test_action_registry` proves the escape is
    shut for the capability that touches the filesystem, and these two are the registry's
    own guards, which nothing else exercises.
    """

    def test_a_ctx_class_that_declared_nothing_answers_over_its_base_s_contract(self):
        """The walk up ``__mro__``, and why it is a walk rather than one lookup: `Ctx`'s
        semantics are taken by subclassing, and a subclass that adds no vocabulary has to
        speak its base's rather than not speak at all."""
        class Quiet(ctx.Ctx):
            pass

        got = ctx.contract_of(Quiet({"fid": "f1"}))
        self.assertIs(got, ctx.contract_of(_ctx_for()))
        self.assertEqual(got.noun, "component")
        self.assertEqual(Quiet({"fid": "f1"}).fid, "f1")

    def test_a_ctx_class_nobody_declared_is_named_rather_than_answered_for(self):
        """Falling back to `Ctx`'s contract would answer a stranger's class in charter's
        words — a refusal naming ``needs`` for a thing that has no ``needs`` — and a bare
        `KeyError` would name a dict instead of the mistake that produced it."""
        with self.assertRaises(TypeError) as e:
            ctx.contract_of(object())
        self.assertIn("object", str(e.exception))
        self.assertIn("declare", str(e.exception))


class SlicesComeFromTheOneSnapshot(unittest.TestCase):
    def test_a_slice_carries_the_snapshot_s_own_content(self):
        c = _ctx_for(needs=("repos", "todos"))
        self.assertEqual([r["name"] for r in c.repos], ["charter", "other"])
        self.assertEqual([t["title"] for t in c.todos], ["one", "two"])

    def test_gather_is_the_whole_scan_because_today_s_renderers_read_it_whole(self):
        c = _ctx_for(needs=("gather",))
        self.assertEqual(c.gather["workspace"], "plane")
        self.assertEqual(c.gather["gathered_at"], 1755000000.0)

    def test_a_list_slice_arrives_as_a_tuple(self):
        """So a component cannot append to, clear, or reorder the list the next
        component in the same repaint is about to read."""
        c = _ctx_for(needs=("repos",))
        self.assertIsInstance(c.repos, tuple)

    def test_the_whole_scan_cannot_be_rewritten_through_the_ctx(self):
        c = _ctx_for(needs=("gather",))
        with self.assertRaises(TypeError):
            c.gather["workspace"] = "elsewhere"

    def test_an_absent_key_degrades_to_empty_rather_than_raising(self):
        """`gather.read` answers `{}` for a frame whose cache has not been written yet,
        and a panel must draw something at that moment rather than fail."""
        c = _ctx_for(needs=("repos", "todos", "gather"), snapshot={})
        self.assertEqual(c.repos, ())
        self.assertEqual(c.todos, ())
        self.assertEqual(dict(c.gather), {})

    def test_a_snapshot_that_is_not_a_mapping_is_refused(self):
        for snap in (None, [], "gathered"):
            with self.subTest(snapshot=snap), self.assertRaises(component.ComponentError):
                _ctx_for(needs=("gather",), snapshot=snap)


class Geometry(unittest.TestCase):
    def test_width_and_height_are_the_pane_s_own(self):
        c = _ctx_for(width=22, height=46)
        self.assertEqual((c.width, c.height), (22, 46))

    def test_the_frame_id_travels_with_the_geometry(self):
        self.assertEqual(_ctx_for(fid="charter-demo-1234").fid, "charter-demo-1234")

    def test_a_geometry_that_is_not_a_count_of_cells_is_refused(self):
        for kw in ({"width": -1}, {"height": -1}, {"width": 1.5}, {"height": "10"},
                   {"width": True}, {"width": None}):
            with self.subTest(**kw), self.assertRaises(component.ComponentError):
                _ctx_for(**kw)

    def test_a_frame_id_that_is_not_text_is_refused(self):
        for fid in (None, 7, ["f1"]):
            with self.subTest(fid=fid), self.assertRaises(component.ComponentError):
                _ctx_for(fid=fid)

    def test_a_collapsed_pane_is_zero_and_not_an_error(self):
        """tmux can leave a pane with no room at all; that is a frame to draw nothing
        in, not a frame to refuse."""
        self.assertEqual(_ctx_for(width=0, height=0).width, 0)


if __name__ == "__main__":                                 # pragma: no cover
    unittest.main()
