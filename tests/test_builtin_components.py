"""Charter's own four panels, expressed as components — and still drawing what they drew.

`frame/builtins.py` is the registry's first consumer and charter's own panels are its
first content, which is the sequencing §4b asks for: the seam a provider will be placed
through is the seam charter's own frame is placed through, so there is no private table of
edges beside the public one.

**The refactor's success condition is that nothing changed**, and these tests are written
to be able to say so rather than to assume it. The pane the `sidebar` composite draws is
compared against its three parts composed; each component's render is compared against the
renderer the slot has always had; and `layout`'s five per-slot tables — which used to be
five hand-written tuples — are compared against the literal geometry charter ships, not
against the registry they are now read from. Reading both sides of an assertion out of the
same constant is the tautology this suite has caught before; the literals are what make
these assertions able to fail.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import instance
from charter.frame import builtins, component, ctx as fctx, gather, layout, slots, state

from tests._isolation import PersonaIso

FID = "f-builtin"


def _row(name, *, branch="main", current=False, repo=None):
    """A `gather`-cache-shaped row — the fields `gather._entry` writes."""
    d = {"name": name, "branch": branch, "dirty": False, "tracked_dirty": False,
         "ahead": 0, "behind": 0, "ci": None, "change": None, "sigil": "",
         "current": current, "worktree_count": 0}
    if repo is not None:
        d["repo"] = repo
    return d


def _change(slug, *, state="unknown", landed=0, total=2):
    """A `gather`-cache-shaped change row — the fields `gather._change_rows` writes.

    Only the four `slots._change_rows` actually draws are needed; the rest of the record
    (`why`, `excluded`, `members`) is what `charter change show` reads, not the sidebar.
    """
    return {"change": slug, "state": state, "landed": landed, "total": total}


class Declared(unittest.TestCase):
    """What the seven components say about themselves.

    No plane needed: `builtins.build()` reads nothing and starts nothing, which is the
    property that lets `layout` ask it at import time.
    """

    def setUp(self) -> None:
        self.reg = builtins.build()

    def test_the_registry_holds_the_seven_charter_draws_in_split_order(self):
        """Split order is registration order, and the three parts of the sidebar are
        registered before the composite that draws them — the registry refuses a
        composite built from parts it has not seen.

        `changes` is the third of those parts, so it is registered with them and not with
        the placed components above: it is a SECTION of the sidebar, not a pane. The
        measurement is `frame/builtins.py`'s own — a placed component has to be in
        `instance.FRAME_SLOTS`, and that list is pinned to agree with the shipped `slots`
        and with `full`, so placing it would put a pane saying "no changes" on every
        operator's frame for a feature most planes never use.

        `chats` and `workspaces` are last, registered and not placed for the same reason
        one measurement further on: a plane with one chat is that same ordinary,
        permanent state, and every placed pane is ~7 of a switch's 41 tmux invocations."""
        self.assertEqual([c.id for c in self.reg.all()],
                         ["identity", "attention", "repos", "personas", "todos",
                          "changes", "sidebar", "chats", "workspaces"])

    def test_only_the_components_charter_places_are_split_for(self):
        """`personas`, `todos` and `changes` are registered and never split for: their
        parent draws them inside its own pane, and a part that appeared on an edge too
        would be drawn twice — once in its own pane and once inside the sidebar's.

        **`chats` and `workspaces` DO declare an edge and are still not placed**, and the
        two kinds of absence are the point. A sidebar part cannot be placed at all — its
        parent draws it. A bar is a pane charter COULD split and deliberately does not, so
        it carries the edge and the size it would take, and a `[[frame.component]]` table
        naming it gets exactly that geometry. What keeps it off every operator's frame is
        `instance.FRAME_SLOTS`, not the registry."""
        placed = {edge: [c.id for c in self.reg.on_edge(edge)]
                  for edge in component.EDGES}
        self.assertEqual(placed, {"top": ["identity", "chats", "workspaces"],
                                  "bottom": ["attention", "repos"],
                                  "left": [], "right": ["sidebar"]})
        for bar in ("chats", "workspaces"):
            self.assertNotIn(bar, builtins.SLOT_OF,
                             f"{bar} became a placed slot — a bar on every operator's "
                             f"frame is the decision `frame/builtins.py` argues against")

    def test_the_sidebar_is_the_composite_of_personas_then_todos_then_changes(self):
        """In the order the pane stacks them, which is not split order — nothing splits
        this pane at all (§4d).

        `changes` is LAST, and `slots._right`'s own comment is why: the order is a
        priority, and each section reserves its blank row out of what the ones above it
        left, so the section that overflows a short column is always the lowest-priority
        one."""
        self.assertEqual([c.id for c in self.reg.children_of("sidebar")],
                         ["personas", "todos", "changes"])
        self.assertEqual([c.id for c in self.reg.children_of("identity")], [])

    def test_each_component_declares_the_edge_and_size_the_frame_draws_it_at(self):
        """The literal geometry charter ships, asserted as literals.

        `right` is 22 columns and the two strips are one row each; the repo table is
        `Content()` because its height is the plane's repo count bounded by what the
        harness may not be charged (`layout.repos_rows`), and a cap written here would be
        a second, weaker copy of that arithmetic.

        `changes` is on `right` with a CAP, which is what says it is a section of the
        sidebar rather than a pane: it is bounded like `todos` and for the same reason —
        a column that is one list has no room to let a second one grow without end.

        The two bars are `Fixed(1)` on `top`, §3.6's own literals: a bar is one row or it
        is nothing, and a `Content()` bar would be a row that appeared and disappeared as
        a sibling chat opened, moving every pane below it."""
        got = {c.id: (c.edge, c.size) for c in self.reg.all()}
        self.assertEqual(got, {
            "identity": ("top", component.Fixed(1)),
            "attention": ("bottom", component.Fixed(1)),
            "repos": ("bottom", component.Content(None)),
            "chats": ("top", component.Fixed(1)),
            "workspaces": ("top", component.Fixed(1)),
            "personas": ("right", component.Fill()),
            "todos": ("right", component.Content(slots._MAX_TODO_LINES)),
            "changes": ("right", component.Content(slots._MAX_CHANGE_LINES)),
            "sidebar": ("right", component.Fixed(22)),
        })

    def test_exactly_one_part_of_the_sidebar_takes_what_is_left(self):
        """`personas` is the `Fill()` — the pane is the persona column everywhere else
        charter names it, so the todos are the section that gives way. The registry
        refuses a second one at registration, which is what makes this a property of the
        arrangement rather than of a tie-break at layout time (§4e)."""
        fills = [c.id for c in self.reg.children_of("sidebar")
                 if isinstance(c.size, component.Fill)]
        self.assertEqual(fills, ["personas"])

    def test_every_need_the_built_ins_declare_is_one_ctx_can_actually_serve(self):
        """A name accepted by `needs` that `ctx` answered with an empty tuple would let a
        component declare it, draw nothing, and pass its own tests against an empty
        fixture — indistinguishable from a plane that genuinely has none, which is #512's
        defect. `component.NEEDS` and `ctx.SERVES` are already asserted against each
        other; this asserts charter's own components stay inside them."""
        for c in self.reg.all():
            with self.subTest(component=c.id):
                for name in c.needs:
                    self.assertIn(name, fctx.SERVES)

    def test_a_composite_declares_everything_its_parts_read(self):
        """The parts draw inside the composite's pane, so whatever they read, the
        composite's own repaint reads. A composite declaring less than its parts would
        understate the cost of the pane it owns — which is the one thing `needs` exists to
        make reviewable."""
        parts = set()
        for child in self.reg.children_of("sidebar"):
            parts |= set(child.needs)
        self.assertTrue(parts <= set(self.reg.get("sidebar").needs),
                        f"sidebar declares {self.reg.get('sidebar').needs} but its parts "
                        f"read {sorted(parts)}")


class OneVocabularyForSlotNames(unittest.TestCase):
    """The slot names and the component ids agree, in both directions.

    Two tables answering "which panels are there" is how the test harness came to keep a
    worse copy of what runs charter (#547). These pin that `builtins.SLOT_OF`,
    `slots.SLOTS` and `instance.FRAME_SLOTS` cannot disagree about the set.
    """

    def test_every_placed_component_names_a_slot_that_has_a_renderer(self):
        self.assertEqual(sorted(builtins.SLOT_OF.values()), sorted(slots.SLOTS))

    def test_every_slot_config_accepts_is_a_component_charter_can_place(self):
        self.assertEqual(sorted(builtins.COMPONENT_OF), sorted(instance.FRAME_SLOTS))

    def test_the_two_directions_of_the_mapping_are_one_table(self):
        """`COMPONENT_OF` is derived from `SLOT_OF`, so a name cannot be added to one and
        forgotten in the other — asserted rather than trusted, because the derivation is
        one line and a future edit could write the second table out by hand."""
        self.assertEqual({slot: cid for cid, slot in builtins.SLOT_OF.items()},
                         builtins.COMPONENT_OF)


class LayoutReadsTheRegistry(unittest.TestCase):
    """`layout`'s five per-slot tables are now one derivation over the components."""

    def test_the_shipped_frame_is_the_geometry_charter_has_always_drawn(self):
        """Literals on the right-hand side, deliberately. Reading these back out of the
        registry would pass against any registry at all, including one that had lost the
        repo table — which is exactly the change #535 shipped and a reviewer, not a test,
        caught."""
        self.assertEqual(layout.SLOT_SIZE,
                         {"top": 1, "bottom": 1, "repos": 1, "right": 22})
        self.assertEqual(layout.SLOT_EDGE,
                         {"top": "top", "bottom": "bottom", "repos": "bottom",
                          "right": "right"})
        self.assertEqual(layout._COLUMN_SLOTS, ("right",))
        self.assertEqual(layout._FIXED_ROW_SLOTS, ("top", "bottom"))
        self.assertEqual(layout.VARIABLE_ROW_SLOTS, frozenset({"repos"}))

    def test_moving_the_table_to_a_side_edge_moves_every_derived_fact_with_it(self):
        """One predicate decides all five, so an arrangement that puts the table on the
        right takes it out of the row tables and into the column one — together. While
        these were five hand-written tuples they agreed only by having been edited in the
        same commit."""
        reg = builtins.build()
        moved = [c if c.id != "repos" else
                 component.Component(id="repos", title="repos", edge="right",
                                     size=c.size, needs=c.needs, render=c.render)
                 for c in reg.all() if c.id in builtins.SLOT_OF]
        got = layout._derive(moved)
        self.assertEqual(got.edge["repos"], "right")
        self.assertEqual(got.column, ("repos", "right"))
        self.assertEqual(got.fixed_rows, ("top", "bottom"))
        self.assertEqual(got.variable_rows, frozenset())

    def test_a_strip_sized_by_its_content_becomes_the_variable_row(self):
        """The other half of the same predicate: `attention` is the variable row the
        moment its size stops being `Fixed`, and leaves the fixed table at the same
        moment. Nothing has to be remembered in two places for that to hold."""
        reg = builtins.build()
        changed = [c if c.id != "attention" else
                   component.Component(id="attention", title="attention", edge="bottom",
                                       size=component.Content(), needs=c.needs,
                                       render=c.render)
                   for c in reg.all() if c.id in builtins.SLOT_OF]
        got = layout._derive(changed)
        self.assertEqual(got.fixed_rows, ("top",))
        self.assertEqual(got.variable_rows, frozenset({"bottom", "repos"}))
        self.assertEqual(got.size["bottom"], 1)     # the floor a size policy already has

    def test_a_wider_sidebar_is_a_wider_sidebar_everywhere(self):
        """`SLOT_SIZE["right"]` is what `repos_cols` insets the table pane by, so a size
        read from one place and not the other is #500's defect. One derivation, one
        number."""
        reg = builtins.build()
        wide = [c if c.id != "sidebar" else
                component.Component(id="sidebar", title="sidebar", edge="right",
                                    size=component.Fixed(30), needs=c.needs,
                                    render=c.render)
                for c in reg.all() if c.id in builtins.SLOT_OF]
        self.assertEqual(layout._derive(wide).size["right"], 30)

    def test_the_split_that_goes_before_the_harness_is_the_one_on_a_before_edge(self):
        """`-b` used to be spelled `slot == "top"`. It is the component's edge now, and
        `bottom` must not acquire one: a `-b` on the attention strip would put it above
        the harness, which is the one thing #488 protected it from."""
        order = ["top", "bottom", "repos", "right"]
        argvs = layout.panel_argvs(slots=order, session="s", socket="/sock",
                                   harness_pane="%0")
        self.assertEqual({slot: ("-b" in argv) for slot, argv in zip(order, argvs)},
                         {"top": True, "bottom": False, "repos": False, "right": False})


class Renders(PersonaIso):
    """Each component draws exactly the panel its slot has always drawn."""

    def setUp(self) -> None:
        super().setUp()
        self.reg = builtins.build()
        for p in ("alpha", "bravo"):
            self.make_persona(p)
        state.record_workspace(FID, "demo")
        gather.save(FID, {
            "gathered_at": 0.0, "workspace": "demo", "current_repo": "charter",
            "repos": [_row("charter", current=True), _row("atlas")],
            "worktrees": [],
            "todos": [{"title": "wire the registry"}, {"title": "measure the frame"}],
            "todo_count": 5,
            # One change, so the sidebar's third section actually draws. An empty list
            # here would leave every assertion about `changes` below satisfied by a
            # section that returns no rows — which is the shape this suite keeps being
            # bitten by, and doubly so here because "no rows when there are none" is the
            # very property that made `changes` a section rather than a pane.
            "changes": [_change("frame-registry", state="blocked", landed=1)],
        })

    def _ctx(self, c, *, width=200, height=20):
        return fctx.build(c.needs, width=width, height=height, fid=FID,
                          snapshot=gather.read(FID))

    def _pane(self, width, height):
        return (mock.patch.object(slots, "_width", lambda: width),
                mock.patch.object(slots, "_height", lambda: height))

    def test_each_component_draws_its_own_slots_panel(self):
        """Joined back together, byte for byte — and the four outputs are pairwise
        distinct, which is what makes this able to fail. A wrapper wired to the wrong slot
        would still satisfy "equals some renderer's output"; it cannot satisfy both."""
        drawn = {}
        for cid, slot in builtins.SLOT_OF.items():
            c = self.reg.get(cid)
            w, h = self._pane(200, 20)
            with w, h:
                drawn[cid] = "\n".join(c.render(self._ctx(c)))
                self.assertEqual(drawn[cid], slots.SLOTS[slot](FID))
        self.assertEqual(len(set(drawn.values())), len(drawn),
                         f"two components drew the same pane: {drawn}")

    def test_a_renderer_that_draws_one_blank_line_still_draws_one(self):
        """``split("\\n")`` and never ``splitlines()``: a panel writes the string out as
        it is, so the adaptation has to round-trip. ``splitlines()`` turns a renderer that
        answered one empty line into a component that answered no lines at all — a pane
        that is blank because it is empty, told apart from a pane that has nothing to
        say only by the number of rows it takes."""
        with mock.patch.dict(slots.SLOTS, {"blank": lambda fid: ""}):
            render = builtins._panel("blank")
            c = self.reg.get("identity")
            self.assertEqual(render(self._ctx(c)), [""])
        with mock.patch.dict(slots.SLOTS, {"two": lambda fid: "a\n\nb"}):
            render = builtins._panel("two")
            c = self.reg.get("identity")
            self.assertEqual(render(self._ctx(c)), ["a", "", "b"])

    def test_the_sidebars_pane_is_exactly_its_three_parts(self):
        """The composite draws the parts, it does not keep a second copy of them. This is
        the whole of the `slots.py` half of this task: `_right` was split into
        `persona_section` and `todo_section` and then composed back, so a pane that
        differed from its parts would mean the split changed what the panel says.

        `changes_section` joined the composition as the third and lowest-priority part.
        Each section's blank row comes OUT of what the sections above it left, which is
        why the budget handed to each one here is the running total rather than the
        pane's own height — a composition that added the rows on top would overflow the
        column by exactly the number of sections in it."""
        w, h = self._pane(22, 20)
        with w, h:
            whole = slots.render("right", FID)
            personas = slots.persona_section(22, 20, terse=False)
            todos = slots.todo_section(FID, 22, 20 - len(personas) - 1, terse=False)
            changes = slots.changes_section(
                FID, 22, 20 - len(personas) - len(todos) - 2)
        self.assertEqual(whole,
                         "\n".join([*personas, "", *todos, "", *changes]))
        self.assertTrue(todos, "fixture drew no todos — the composition is untested")
        self.assertTrue(changes,
                        "fixture drew no changes — the third part is untested, and an "
                        "empty one composes identically to not being there at all")

    def test_a_terse_sidebar_spends_fewer_rows_on_todos_than_a_normal_one(self):
        """The density cap moved out of `_right` and into `todo_section` with this task,
        and nothing in the suite was holding it: mutating it away (`min(budget,
        _MAX_TODO_LINES)` regardless of density) left 254 tests green. A guard that moves
        gets a test at the place it moved to, or the move is where it silently stops
        working.

        Asserted as the two densities differing, and as the terse count being
        `slots._TERSE_ROWS`, on a pane with rows to spare — so this is the CAP being
        applied and not the pane running out."""
        gather.save(FID, {
            "gathered_at": 0.0, "workspace": "demo", "current_repo": None,
            "repos": [], "worktrees": [],
            "todos": [{"title": f"todo {i}"} for i in range(12)], "todo_count": 12,
        })
        with self._pane(22, 30)[0], self._pane(22, 30)[1]:
            normal = slots.todo_section(FID, 22, 20, terse=False)
            terse = slots.todo_section(FID, 22, 20, terse=True)
        self.assertEqual(len(terse), slots._TERSE_ROWS)
        self.assertEqual(len(normal), slots._MAX_TODO_LINES)
        self.assertLess(len(terse), len(normal))

    def test_a_component_reads_the_plane_exactly_where_it_said_it_would(self):
        """`needs` is a claim about what the renderer reads, and this is what checks it.

        Every component is drawn with `gather`'s two readers recording, and the recording
        must be non-empty for exactly the components that declared ``gather``. A
        declaration that drifted from its renderer — either way — is red: a component that
        quietly started reading the plane cache without declaring it, and one that
        declares a cost it does not pay, are both defects, and only the first is obvious.
        """
        declared = {cid for cid in builtins.SLOT_OF
                    if "gather" in self.reg.get(cid).needs}
        self.assertTrue(declared and declared != set(builtins.SLOT_OF),
                        "the fixture must have components on both sides of this")
        for cid, slot in builtins.SLOT_OF.items():
            c = self.reg.get(cid)
            seen: list[str] = []
            real_read, real_cached = gather.read, gather.cached

            def rec_read(*a, _f=real_read, _s=seen, **k):
                _s.append("read")
                return _f(*a, **k)

            def rec_cached(*a, _f=real_cached, _s=seen, **k):
                _s.append("cached")
                return _f(*a, **k)

            # Built BEFORE the recorders go on: `ctx.build` reads the snapshot itself,
            # and a recorder that counted the harness's own read would report every
            # component as touching the cache.
            cx = self._ctx(c)
            w, h = self._pane(200, 20)
            with w, h, mock.patch.object(gather, "read", rec_read), \
                    mock.patch.object(gather, "cached", rec_cached):
                out = c.render(cx)
            with self.subTest(component=cid):
                self.assertTrue(out, f"{cid} drew nothing — the reading is untested")
                self.assertEqual(bool(seen), "gather" in c.needs,
                                 f"{cid} declares needs={c.needs} and touched "
                                 f"{seen or 'nothing'}")

    def test_the_sidebars_own_parts_read_where_they_said_they_would(self):
        """The same check for the three components that are never placed on an edge — they
        are drawn by their parent, so nothing else would ever exercise their declarations.
        """
        for cid in ("personas", "todos", "changes"):
            c = self.reg.get(cid)
            seen: list[str] = []
            real_read = gather.read

            def rec_read(*a, _f=real_read, _s=seen, **k):
                _s.append("read")
                return _f(*a, **k)

            cx = fctx.build(c.needs, width=22, height=10, fid=FID,
                            snapshot=gather.read(FID))
            w, h = self._pane(22, 20)
            with w, h, mock.patch.object(gather, "read", rec_read):
                out = c.render(cx)
            with self.subTest(component=cid):
                self.assertTrue(out, f"{cid} drew nothing — the reading is untested")
                self.assertEqual(bool(seen), "gather" in c.needs,
                                 f"{cid} declares needs={c.needs} and touched "
                                 f"{seen or 'nothing'}")


if __name__ == "__main__":
    unittest.main()
