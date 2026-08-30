"""A component id is what the frame is made of, end to end — config to painted pane.

Phase 1 built the registry, the provider seam and the standin, and then found that none
of it could reach a pane: `Registry.place` had **zero production callers**, because every
step between a committed `[[frame.component]]` table and a `split-window` spoke the four
committed SLOT NAMES instead. A provider could be placed and never drawn, which is the
one property §4b exists to deliver.

**These cases are the chain, one link each**, and they are written against a real
installed distribution (`tests.test_component_providers._SitePackages` — a real
`.dist-info`, a real `entry_points.txt`, a real module on `sys.path`) rather than a
stubbed `entry_points`, for the reason that file's own docstring gives: a stub proves the
stub works.

* `instance.component_tables` **places** the provider's component, honouring the
  committed rectangle (§4i: arrangement is committed, execution is local).
* `commands_frame._drawable_slots` **keeps** it, where `slots.unimplemented` used to drop
  every name `frame/slots.py` has no renderer for.
* `layout` **sizes and splits** it — the `-h`/`-v`, the `-b` and the `-l` come off the
  component's own edge and the committed size, not off a table of four names.
* `charter panel <component-id>` **draws** it, in a panel process that builds its own
  registry. That is the link the coordinator's note calls the hardest, and it is why
  `panel.run` takes a component NAME now rather than a key of `slots.SLOTS`.

**Nothing here asserts about the machine's own providers.** Every case asks about an id
its own fixture installed, so a laptop carrying a real charter component provider gives
the answer CI gives.

**And the committed spelling still resolves to the frame it always did.** `[frame] slots`
is shorthand for four built-in ids now, and `CharterOwnConfigIsUnchanged` reads charter's
OWN `charter.toml` off disk and pins the whole resolved arrangement — because a change
that silently removed the repo table from charter's own plane shipped once (#535) and was
caught by a reviewer, not by a test.

**The classes below `CharterOwnConfigIsUnchanged` pin the branch's GUARDS**, one case per
line that refuses, clamps, contains or falls back. Each was written by deleting the line,
running the whole suite and watching it stay green — twelve of them did, and every one of
those twelve is a `if`/fallback/`except` that only fires for a component charter did not
write, or for a value that is not text. The cases above could not see any of them, and
the reason is the same one every time: **the payload passed either way.** The three
drawing cases all draw eleven columns into a forty-column pane, so the pane's real width
is never used; `_derive` is only ever handed charter's own components, which have a
committed spelling for the fallback to skip; `slot_sizes` is called by one case, on a
frame with no provider in it. A test that cannot fail is not a pin, and this repo has
shipped that kind believing it was the other.

**And the paragraph above was itself a claim, not a measurement, the first time it was
written.** The commit that added those twelve cases said *every* guard this branch adds is
a line a test goes red without, and left six more of its own standing: `run`'s
unknown-slot refusal loses its clip and stderr grows from 227 bytes to 5071; `_placed_here`
— a function this branch added whole — has both halves of its only guard, its read of
`config.FRAME` and the `_policy_cells` fallback below it survive deletion; and
`panel_argvs`' last resort emits `-l None` to `split-window` when it is taken out. The
last four classes here are those six, and the same sweep was run on every one of them
before this sentence was written rather than after. The lesson is the sweep's scope: it
has to cover the lines the FIX added, not only the lines the review named.
"""

from __future__ import annotations

import io
import os
import pathlib
import sys
import tempfile
import tomllib
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest import mock

from charter import commands_frame, config, contain, instance, tui
from charter.frame import builtins, component, layout, panel, slots

from tests._isolation import PersonaIso
from tests.test_component_providers import CID, ENTRY, MODULE, _SitePackages, _source

#: charter's own committed file, read off disk rather than through `config` — which
#: resolves a WORKTREE back to the main tree it was cut from and would therefore test the
#: operator's checkout instead of this one. Same reason `tests/test_frame_config.py`
#: reads it this way.
_COMMITTED = pathlib.Path(__file__).resolve().parents[1] / "charter.toml"

#: What the provider below draws, and a string no built-in renderer could produce.
_DREW = "metrics 42"


def _installed(case, *, render: str = f"lambda ctx: [{_DREW!r}]", head: str = "",
               needs: tuple[str, ...] = ()) -> None:
    """Put one real provider distribution supplying :data:`CID` on ``sys.path``."""
    site = _SitePackages(case)
    site.install("acme-charter", "1.0", {CID: ENTRY},
                 {MODULE: _source(render=render, head=head, needs=needs)})
    return site


def _painted(name: str, fid: str = "f-1", *, cols: int = 40, rows: int = 6):
    """`charter panel <name>` for one pass, and what it actually put on the pane.

    The pane's own tty is faked rather than inherited: `slots._width` measures
    `os.get_terminal_size(sys.stdout.fileno())`, and a captured `StringIO` has no fileno
    at all — the same fixture `tests/test_frame_panel.py` uses for exactly this.
    """
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))):
            rc = panel.run(name, fid, once=True)
    painted = out.getvalue()
    return rc, painted.split("\x1b[2J", 1)[-1], err.getvalue()


class APanelDrawsAProvidersComponent(PersonaIso, unittest.TestCase):
    """The link Phase 1 could not build: a panel process finding a component from an id.

    A panel builds no registry today and knows no providers — it looks its argument up in
    `slots.SLOTS`, a dict of four functions charter wrote. So `charter panel acme.metrics`
    was `unknown slot 'acme.metrics'` however correct everything upstream of it was.
    """

    def setUp(self):
        super().setUp()
        _installed(self)

    def test_charter_panel_draws_an_installed_providers_component(self):
        """The whole chain's last link, and the task's own failing test."""
        rc, painted, _err = _painted(CID)
        self.assertEqual(rc, 0, painted)
        self.assertIn(_DREW, painted)

    def test_the_provider_is_imported_only_because_a_panel_asked_for_it(self):
        """§4b's laziness, still true: nothing imports a provider to answer a question
        about which components exist. The module arrives when the pane does."""
        self.assertFalse(builtins.supplies("not.installed"))
        self.assertTrue(builtins.supplies(CID))
        self.assertNotIn(MODULE, sys.modules,
                         "asking whether a provider is installed imported it")
        rc, painted, _err = _painted(CID)
        self.assertEqual(rc, 0, painted)
        self.assertIn(MODULE, sys.modules)

    def test_a_component_that_raises_costs_its_own_pane_and_names_itself(self):
        """§4b property 1, through `panel.run` rather than through `Registry.draw`
        directly: the pane says which component failed, and the panel does not exit."""
        _installed(self, render="lambda ctx: 1 / 0")
        rc, painted, _err = _painted(CID)
        self.assertEqual(rc, 0, painted)
        self.assertIn(CID, painted)
        self.assertIn("ZeroDivisionError", painted)

    def test_a_provider_charter_cannot_load_is_a_pane_that_says_so(self):
        """§4b property 4. An installed distribution whose module raises on import is
        the case a standin exists for, and it must reach a PANE — a message with nowhere
        to appear is the silent drop #512 and #535 both are."""
        _installed(self, head="raise RuntimeError('provider is broken')")
        rc, painted, _err = _painted(CID, cols=78)
        self.assertEqual(rc, 0, painted)
        self.assertIn("acme-charter", painted)
        self.assertIn("provider is broken", painted)

    def test_a_bare_name_no_component_answers_to_is_still_refused(self):
        """The refusal that must survive the widening: `charter panel sideways` is a
        typo, there is nothing to stand in for, and a pane drawn for it would be the
        permanently-dead rectangle `_drawable_slots` exists to prevent."""
        rc, painted, err = _painted("sideways")
        self.assertNotEqual(rc, 0)
        self.assertIn("sideways", painted)
        self.assertIn("sideways", err)


class ABuiltInIsReachableByItsComponentIdToo(PersonaIso, unittest.TestCase):
    """`top` is `identity`'s committed spelling, not a second thing.

    The four slot names stay because they are committed — in `[frame] slots` on every
    plane that has a charter.toml — but they are shorthand for built-in ids now, and the
    id is what the frame reasons with. Both spellings must reach the same renderer, or
    there are two vocabularies rather than one with an alias table.
    """

    def test_the_component_id_draws_what_the_slot_name_draws(self):
        for cid, slot in builtins.SLOT_OF.items():
            with self.subTest(cid=cid):
                by_id = _painted(cid)
                by_slot = _painted(slot)
                self.assertEqual(by_id[0], 0, by_id[1])
                self.assertEqual(by_id[1], by_slot[1])

    def test_a_built_in_whose_renderer_is_missing_is_refused_by_either_spelling(self):
        """Charter's own components never reach the provider path, so removing a
        renderer refuses both spellings rather than sending one of them off to look for
        an installed distribution called `sidebar`."""
        with mock.patch.dict(slots.SLOTS):
            del slots.SLOTS["right"]
            for name in ("right", "sidebar"):
                with self.subTest(name=name):
                    rc, painted, _err = _painted(name)
                    self.assertNotEqual(rc, 0, painted)

    def test_a_distribution_claiming_a_built_in_id_does_not_answer_for_it(self):
        """The same refusal with an adversary present, which is the only version of it
        that is a guard. An installed distribution declaring `sidebar` must not become
        the answer to a question about charter's OWN component: the panel would take the
        built-in path anyway and paint `unknown slot right` at rc 0, which is the
        convincing empty rather than the refusal.

        Asked with the renderer removed because that is the only state in which the two
        branches disagree — with `slots.SLOTS` whole, `right` is drawable either way and
        the case would pass against a `drawable` that had no such guard at all.
        """
        site = _SitePackages(self)
        site.install("acme-charter", "1.0", {"sidebar": ENTRY},
                     {MODULE: _source(cid="sidebar")})
        self.assertTrue(builtins.supplies("sidebar"))
        with mock.patch.dict(slots.SLOTS):
            del slots.SLOTS["right"]
            for name in ("right", "sidebar"):
                with self.subTest(name=name):
                    self.assertFalse(slots.drawable(name))
                    rc, painted, _err = _painted(name)
                    self.assertNotEqual(rc, 0, painted)


class ConfigPlacesAProvider(unittest.TestCase):
    """`[[frame.component]]` naming a provider resolves to a placement, rectangle and all.

    Until now `instance.component_tables` refused any `use` outside `builtins.SLOT_OF`,
    so the arrangement §4b's own example writes could not be expressed at all.
    """

    def setUp(self):
        _installed(self)
        self.cfg = {"frame": {"component": [
            {"use": "identity"},
            {"use": "attention"},
            {"use": CID, "edge": "right", "size": 12},
        ]}}

    def test_the_provider_is_one_of_the_placements(self):
        got = instance.frame_components(self.cfg)
        self.assertEqual([p["use"] for p in got], ["identity", "attention", CID])

    def test_the_committed_rectangle_is_what_is_placed(self):
        """§4i's own finding, at the config boundary this time: the provider declares
        `right`/`Fixed(12)` and the table says `right`/`12`, so this case would pass
        against either. The next one is what tells them apart."""
        got = instance.frame_components(self.cfg)[-1]
        self.assertEqual(got["edge"], "right")
        self.assertEqual(got["size"], component.Fixed(12))

    def test_a_table_that_moves_the_provider_moves_it(self):
        """The provider's module declares `edge="right", size=Fixed(12)`. A committed
        table asking for `bottom`/`4` gets `bottom`/`4` — execution does not decide
        arrangement, which is the inversion §4i measured in `Registry.place`."""
        cfg = {"frame": {"component": [{"use": CID, "edge": "bottom", "size": 4}]}}
        got = instance.frame_components(cfg)[-1]
        self.assertEqual((got["edge"], got["size"]), ("bottom", component.Fixed(4)))

    def test_the_name_that_travels_downstream_is_the_component_id(self):
        """`frame_of` answers the names `layout` splits panes for. A built-in travels as
        its committed slot name — `charter panel top` is an argv that exists on every
        plane — and a provider, which has no committed spelling, travels as its id."""
        self.assertEqual(instance.frame_of(self.cfg)["slots"],
                         ["top", "bottom", CID])

    def test_the_rectangle_travels_with_it_and_not_only_the_name(self):
        """The names alone are what `slots` already carried. A component charter did not
        write also has an EDGE and a SIZE that nothing else on this machine knows, and
        `frame_of`'s answer is the only thing `layout._placed_here` gets to read —
        `config.FRAME` is this function's return value. Dropped here, the provider's pane
        is split at whatever `SLOT_SIZE` happens to hold, which is a `KeyError`."""
        got = instance.frame_of(self.cfg)["components"]
        self.assertEqual([(p["slot"], p["edge"], p["size"]) for p in got],
                         [("top", "top", component.Fixed(1)),
                          ("bottom", "bottom", component.Fixed(1)),
                          (CID, "right", component.Fixed(12))])

    def test_a_provider_no_installed_distribution_supplies_refuses_the_arrangement(self):
        """Unchanged, and deliberately: charter cannot honour a rectangle for a component
        it cannot even find, and #535 is why an arrangement is refused WHOLE rather than
        one table at a time."""
        cfg = {"frame": {"component": [{"use": "nobody.supplies", "edge": "right",
                                        "size": 12}]}}
        self.assertIsNone(instance.component_tables(cfg["frame"]))

    def test_a_provider_placed_without_a_rectangle_refuses_the_arrangement(self):
        """Charter does not import a stranger's code to answer a geometry question on
        every command — `config.FRAME` is resolved by `charter --version` too. So a
        provider's placement carries its own edge and size, or it is not one charter can
        resolve without running the provider, and the arrangement is refused."""
        for table in ({"use": CID}, {"use": CID, "edge": "right"},
                      {"use": CID, "size": 12}):
            with self.subTest(table=table):
                self.assertIsNone(
                    instance.component_tables({"component": [table]}))

    def test_a_rectangle_charter_cannot_honour_refuses_the_arrangement(self):
        """A key that is PRESENT and unusable, which the absent-key case above never
        reaches — and the two are separate cases because the consequences are not the
        same shape.

        A bad ``size`` is the expensive one. ``Fixed.__post_init__`` calls
        `component.cells`, which refuses `0`, a negative, a `str` and — explicitly —
        a `bool`, since `isinstance(True, int)` would otherwise make ``size = true``
        mean ``Fixed(1)``. That raise would come out of `component_tables` →
        `frame_of` → `config.derive`, and `derive` resolves ``FRAME`` **outside** the
        try/except that catches a malformed charter.toml. So the whole of
        `import charter.config` dies, and every command on that clone dies with it —
        `charter --version` included. `test_a_committed_rectangle_charter_cannot_honour
        _still_leaves_charter_runnable` below pins that consequence end to end.

        A bad ``edge`` is quieter and still wrong: unvalidated, ``"sideways"`` is placed
        and travels into `layout._edge_of`, falls out of `_COLUMN_EDGES`/`_ROW_EDGES`/
        `_BEFORE_EDGES` and silently becomes a plain `-v` after-split — a pane on an
        edge nobody asked for.
        """
        for table in ({"use": CID, "edge": "right", "size": 0},
                      {"use": CID, "edge": "right", "size": True},
                      {"use": CID, "edge": "right", "size": -4},
                      {"use": CID, "edge": "right", "size": "12"},
                      {"use": CID, "edge": "sideways", "size": 12},
                      {"use": CID, "edge": "", "size": 12}):
            with self.subTest(table=table):
                self.assertIsNone(
                    instance.component_tables({"component": [table]}))


class ABrokenRectangleCostsTheFrameAndNotTheCLI(unittest.TestCase):
    """The consequence of the guard above, asked of `config.derive` itself.

    `derive` is what runs at `import charter.config`, which is to say on **every**
    charter command. It wraps `instance.load` in a try/except so a malformed
    charter.toml degrades instead of raising — but `FRAME` is resolved after that block,
    not inside it, so anything `frame_of` raises is uncatchable and terminal. A
    committed `[[frame.component]]` table arrives from someone else's machine (the
    containment rule in README.md), so "committed" is not "trusted".

    **The good-value case is the control, and it is load-bearing.** Without it, a run
    where the fixture's distribution was not visible to `derive` would answer the
    defaults for the reason the mutation is supposed to be caught by — `supplies()`
    saying no, long before any size is read — and the refusal case would stay green
    with the guard deleted. It is asked FIRST for the same reason.
    """

    def setUp(self):
        _installed(self)
        self._tmp = tempfile.TemporaryDirectory(prefix="charter-plane-")
        self.addCleanup(self._tmp.cleanup)
        self.root = pathlib.Path(self._tmp.name)

    def _derived(self, size: str) -> dict:
        """``config.derive`` against a plane whose committed table asks for *size*."""
        (self.root / "charter.toml").write_text(
            f'schema = 1\n\n[[frame.component]]\nuse = "{CID}"\n'
            f'edge = "right"\nsize = {size}\n', encoding="utf-8")
        return config.derive(self.root)["FRAME"]

    def test_a_committed_rectangle_charter_can_honour_is_the_frame_it_derives(self):
        frame = self._derived("12")
        self.assertEqual(frame["slots"], [CID])
        self.assertEqual([(p["slot"], p["edge"], p["size"]) for p in frame["components"]],
                         [(CID, "right", component.Fixed(12))])

    def test_a_committed_rectangle_charter_cannot_honour_still_leaves_charter_runnable(self):
        for size in ("0", "true", "-4", '"12"'):
            with self.subTest(size=size):
                frame = self._derived(size)      # must not raise: `charter --version`
                self.assertEqual(frame["slots"], instance.FRAME_DEFAULTS["slots"])
                self.assertEqual(frame["components"], [])


class TheLauncherSplitsAPaneForIt(unittest.TestCase):
    """A placed provider survives every filter between config and `split-window`.

    **The frame under test is `instance.frame_of`'s own answer, not one assembled here.**
    `config.FRAME` is literally that function's return value (`config.py`), so an
    arrangement built by hand into the shape `layout` wants would be this file
    manufacturing the condition it claims to observe — and would still pass with
    `frame_of` dropping the placements on the floor, which is how a provider's pane comes
    to be split at a size nothing chose.
    """

    def _frame(self, *tables: dict) -> dict:
        return instance.frame_of({"frame": {"component": list(tables)}})

    def setUp(self):
        _installed(self)
        self.frame = self._frame({"use": "identity"}, {"use": "attention"},
                                 {"use": CID, "edge": "right", "size": 12})
        self.assertEqual(self.frame["slots"], ["top", "bottom", CID])

    def test_the_unimplemented_filter_keeps_a_component_a_provider_supplies(self):
        """`slots.unimplemented` asks "is there a renderer for this", and the answer for
        a provider's component is not in `slots.SLOTS`. Left as it was, this filter
        dropped every provider before a pane was ever split for it."""
        self.assertEqual(slots.unimplemented(["top", CID]), [])
        self.assertEqual(slots.unimplemented(["top", "made.up"]), ["made.up"])

    def test_it_is_drawable_and_keeps_its_place_in_the_split_order(self):
        with mock.patch.dict(config.FRAME, self.frame):
            self.assertEqual(commands_frame._drawable_slots(200, 50),
                             ["top", "bottom", CID])

    def test_the_split_carries_the_committed_size_and_the_components_own_direction(self):
        with mock.patch.dict(config.FRAME, self.frame):
            argvs = layout.panel_argvs(slots=["top", "bottom", CID], session="f-1",
                                       socket="/sock", harness_pane="%0")
        cmd = argvs[-1]
        self.assertIn("-h", cmd, cmd)           # `right` costs columns
        self.assertNotIn("-b", cmd, cmd)        # and is placed after the harness
        self.assertEqual(cmd[cmd.index("-l") + 1], "12")
        self.assertEqual(cmd[-3:], [CID, "--session", "f-1"])

    def test_the_repo_table_is_inset_by_a_provider_split_before_it(self):
        """#500's arithmetic, which reads a per-slot table of four names. A 12-column
        component on `right` narrows the pane the table is then carved out of by 12 plus
        its border — or the table is sized for a pane it does not have."""
        with mock.patch.dict(config.FRAME, self.frame):
            self.assertEqual(layout.repos_cols([CID, "repos"], window_cols=200),
                             200 - 12 - layout._BORDER_COLS)

    def test_a_provider_on_a_row_edge_charges_the_table_its_rows(self):
        frame = self._frame({"use": CID, "edge": "top", "size": 3})
        with mock.patch.dict(config.FRAME, frame):
            got = layout.repos_rows(content_rows=99, window_rows=50,
                                    slots=[CID, "repos"])
            self.assertEqual(got, 50 - (3 + layout._BORDER_ROWS)
                             - layout._BORDER_ROWS - layout.HARNESS_MIN_ROWS)


class TheRespawnHookStillOnlyArmsNamesCharterCanDraw(unittest.TestCase):
    """The guard that widened, and the half of it that must not.

    `_panel_died_hook_argv` interpolates the component name into tmux CONFIG TEXT, which
    is the `[frame] hotkey` class — a newline there once achieved code execution at
    launch. So the widening is to names charter can actually resolve to a component,
    never to a SHAPE: `top.` is namespaced-looking and is refused exactly as it was, and
    so is `acme.other` on a machine whose only provider supplies `acme.metrics`.

    `tests/test_frame_launcher.py::PanelRespawnHook` pins the other three clauses and
    pins each so that no OTHER clause could be what refused it; every name here is one
    bare word that passes `_action_word_is_safe`, for the same reason.
    """

    def _armed(self, name):
        """The real builder, with the arguments a launch hands it."""
        return commands_frame._panel_died_hook_argv(
            socket="charter", panel_pane="%11", slot=name, fid="demo-1234")

    def test_a_component_an_installed_provider_supplies_is_armed(self):
        _installed(self)
        self.assertIsNotNone(self._armed(CID))

    def test_a_name_that_merely_looks_namespaced_is_not(self):
        _installed(self)
        for hostile in ("top.", "acme.other", "acme.metrics.x", "middle"):
            with self.subTest(hostile=hostile):
                self.assertIsNone(self._armed(hostile))


#: The slots charter SHIPS, written out — the literal `CharterOwnConfigIsUnchanged` keeps
#: on purpose, and the one every case in it filters this plane's answer down to.
#:
#: A LITERAL here and not `instance.FRAME_SLOTS`, unlike
#: `test_frame_config.CharterOwnPlaneDrawsEveryEdgeItShips`, and the two want different
#: things. That class asks *does this plane still draw every edge charter ships* — a
#: question about a set that MOVES, so it has to read the set. This class asks *does the
#: frame still resolve to what it resolved to* across a refactor, so a change to the
#: shipped four SHOULD come and update this line by hand. Both reds are wanted; only the
#: red an operator's own `[[frame.component]]` used to cause was not (#701).
_SHIPPED_SLOTS = ["top", "bottom", "repos", "right"]


class CharterOwnConfigIsUnchanged(unittest.TestCase):
    """charter's own committed `charter.toml` resolves to the frame it always drew.

    #535 removed the repo table from charter's own plane and a reviewer caught it, not a
    test. This reads the file off disk and pins the whole resolved arrangement — the slot
    list, the component ids behind it, and every rectangle — so a refactor that speaks a
    new vocabulary has to keep answering in the old one.

    **Pins what charter ships, over the plane's answer — never the plane's answer whole
    (#701).** Every case below drops what `_SHIPPED_SLOTS` does not name and compares
    what is left, so charter's four keep their ids, their edges, their sizes and their
    split order, and this plane may also place a component of its own between them.
    Asserted whole, these four were a claim that charter's own operator may not use
    `[[frame.component]]` for anything charter did not ship — which is the feature this
    class's own file exists to place. Adding either bar `docs/frame.md` documents turned
    all four red, and #661 is the same defect one layer down: a read of the committed
    file that turned a legal arrangement into a failure.

    What survives the filter is the whole property: a refactor that loses `repos`, moves
    it off `bottom`, hides it, re-keys it or splits it after the sidebar still reddens
    every case it used to.
    """

    def setUp(self):
        self.cfg = tomllib.loads(_COMMITTED.read_text(encoding="utf-8"))

    def test_the_slot_list_is_the_one_that_is_committed(self):
        """The shipped four, in the shipped order, as a SUBSEQUENCE of what this plane
        resolves — *drop what charter does not ship, then compare*, which carries the
        membership and the order in one assertion and still names the whole list on a
        failure."""
        got = instance.frame_of(self.cfg)["slots"]
        self.assertEqual([s for s in got if s in _SHIPPED_SLOTS], _SHIPPED_SLOTS, got)

    def test_the_arrangement_it_declares_resolves_to_the_frame_it_always_drew(self):
        """This plane writes its arrangement out, and the tables must still answer the
        shipped rectangle for every component.

        It used to write `slots` and declare no tables, and this case asserted exactly
        that. The vocabulary changed when the plane took `bg`/`pad`/`key`, which `slots`
        cannot express — so the case now asserts the thing that was always the point:
        **a new vocabulary has to keep answering in the old one.** The frame is
        unchanged, and the three cases either side of this one are what say so.

        The order is asserted here too, because the list is the SPLIT order and therefore
        the geometry (#488/#500): `repos` before `right` draws the table at the full
        window width from 95 columns up; reversed, it is split off a harness the sidebar
        has already narrowed and needs 118. That reversal is exactly what this class
        caught when the tables were first written.

        `assertIsNotNone` is the load-bearing line and is not a formality: an arrangement
        is refused WHOLE (#535), and this plane's `slots` list resolves to the same four
        panels its tables do — so a file whose arrangement charter can no longer read
        falls back to `slots` and every other assertion in this class goes on passing.
        That silent fallback is how #690 shipped a documented snippet that turned the
        frame off.

        The edge and visibility are asked of the shipped four only. A component with no
        committed slot name is in no derived table at all — `SLOT_EDGE` would `KeyError`
        on `chats` — because its rectangle is read back off this arrangement instead
        (`layout._placed_here`, #687); and whether the plane's OWN component starts
        hidden is the operator's line to write, not charter's to pin."""
        got = instance.component_tables(self.cfg.get("frame"))
        self.assertIsNotNone(got, "this plane declares its arrangement")
        shipped = [c for c in got if c["slot"] in _SHIPPED_SLOTS]
        self.assertEqual([c["slot"] for c in shipped], _SHIPPED_SLOTS,
                         [c["slot"] for c in got])
        for c in shipped:
            with self.subTest(use=c["use"]):
                self.assertEqual(c["edge"], layout.SLOT_EDGE[c["slot"]])
                self.assertEqual(c["visible"], True)

    def test_every_placement_is_the_built_in_it_always_was(self):
        """Filtered by the component ID rather than by the whole tuple: a `repos` this
        plane moved, hid or re-keyed stays in the filtered list and reddens the
        comparison naming itself, instead of being dropped as *not one of ours* and
        reported as a length that does not match."""
        want = [("identity", "top", "top", True),
                ("attention", "bottom", "bottom", True),
                ("repos", "repos", "bottom", True),
                ("sidebar", "right", "right", True)]
        shipped = {use for use, _, _, _ in want}
        got = [(p["use"], p["slot"], p["edge"], p["visible"])
               for p in instance.frame_components(self.cfg)]
        self.assertEqual([t for t in got if t[0] in shipped], want, got)

    def test_the_frame_it_splits_is_byte_for_byte_the_frame_it_split(self):
        """The whole launch argv, not a summary of it. `slot_sizes` and `panel_argvs`
        are where a re-keyed table would show up first, and they are asserted against
        literals rather than against the tables under test.

        `panel_argvs` is one `split-window` per slot in the order it was handed, which is
        what `zip(…, strict=True)` says out loud — so pairing each argv with its slot and
        then keeping the shipped ones asserts the argv AND its position, and a length
        that stopped matching is a `ValueError` here rather than four argvs silently
        compared against the wrong four slots."""
        f = instance.frame_of(self.cfg)
        with mock.patch.dict(config.FRAME, f):
            sizes = layout.slot_sizes(f["slots"], window_rows=50, content_rows=6)
            self.assertEqual({s: n for s, n in sizes.items() if s in _SHIPPED_SLOTS},
                             {"top": 1, "bottom": 1, "repos": 6, "right": 22}, sizes)
            argvs = layout.panel_argvs(slots=f["slots"], session="f-1", socket="/sock",
                                       harness_pane="%0", sizes=sizes)
        split = [(slot, a[a.index("split-window") + 1:])
                 for slot, a in zip(f["slots"], argvs, strict=True)]
        self.assertEqual([p for p in split if p[0] in _SHIPPED_SLOTS], [
            ("top", ["-t", "%0", "-v", "-b", "-l", "1", "-P", "-F", "#{pane_id}", "--",
                     *layout.panel_command(slot="top", session="f-1")]),
            ("bottom", ["-t", "%0", "-v", "-l", "1", "-P", "-F", "#{pane_id}", "--",
                        *layout.panel_command(slot="bottom", session="f-1")]),
            ("repos", ["-t", "%0", "-v", "-l", "6", "-P", "-F", "#{pane_id}", "--",
                       *layout.panel_command(slot="repos", session="f-1")]),
            ("right", ["-t", "%0", "-h", "-l", "22", "-P", "-F", "#{pane_id}", "--",
                       *layout.panel_command(slot="right", session="f-1")]),
        ])


class TheStackIsSizedInTheUnitEachPaneWasDeclaredIn(unittest.TestCase):
    """`slot_sizes` answers for a component this plane placed, and a pane that costs
    COLUMNS is charged the harness no ROWS.

    Both halves were correct and neither was pinned, for the same reason: every existing
    case that calls either function calls it on a frame with no provider in it —
    `CharterOwnConfigIsUnchanged` reads charter's own committed file, which places four
    built-ins and nothing else, and the four are already in `layout`'s shipped tables. A
    per-slot table keyed by four names answers those four correctly whatever it does with
    a fifth.

    `commands_frame._reassert_sizes` calls both on every window resize and every density
    relayout, so what is measured here is not a startup path.
    """

    def setUp(self):
        _installed(self)
        self.frame = instance.frame_of({"frame": {"component": [
            {"use": "identity"}, {"use": "attention"}, {"use": "repos"},
            {"use": CID, "edge": "right", "size": 12}]}})
        self.assertEqual(self.frame["slots"], ["top", "bottom", "repos", CID])

    def _sizes(self) -> dict[str, int]:
        with mock.patch.dict(config.FRAME, self.frame):
            return layout.slot_sizes(self.frame["slots"], window_rows=50, content_rows=6)

    def test_a_component_this_plane_placed_is_in_the_map_at_its_committed_size(self):
        """Dropped from the map, the provider is a pane the launcher splits at nothing
        and `_reassert_sizes` never re-asserts — and every other slot is then sized
        against a stack one pane short."""
        self.assertEqual(self._sizes(), {"top": 1, "bottom": 1, "repos": 6, CID: 12})

    def test_the_harness_is_charged_no_rows_for_a_pane_that_costs_columns(self):
        """The sidebar's 12 are COLUMNS. Charged as rows they come straight off the
        harness — 39 rows becomes 26 in this 50-row window — and that number is what
        `resize-pane -y` is given, so the frame is wrong on screen and not only in a
        dict. Asked twice: against the arithmetic, and against the same map with the
        provider taken out of it, because a column edge that costs rows is exactly the
        difference between those two answers.
        """
        sizes = self._sizes()
        with mock.patch.dict(config.FRAME, self.frame):
            got = layout.harness_rows(sizes, window_rows=50)
            without = layout.harness_rows(
                {k: v for k, v in sizes.items() if k != CID}, window_rows=50)
        rows = sum(sizes[s] + layout._BORDER_ROWS for s in ("top", "bottom", "repos"))
        self.assertEqual(got, 50 - rows)
        self.assertEqual(got, without,
                         "a pane on a column edge changed the harness's row count")


class LayoutDerivesEveryFactFromTheComponentItself(unittest.TestCase):
    """`_derive` is where all five per-slot tables come from, and it must survive being
    shown a component charter did not write.

    Its own docstring calls the fallback on that line "the line Phase 1 could not cross":
    it used to be `SLOT_OF[c.id]`, a `KeyError` by design for any id charter has no
    committed spelling for. The three existing callers in `test_builtin_components.py`
    all hand it charter's own components, which have one — so the tables they check are
    built by the branch that was already there.
    """

    def _metrics(self):
        return component.Component(id=CID, title="Metrics", edge="right",
                                   size=component.Fixed(12), needs=(), events=(),
                                   render=lambda ctx: [_DREW])

    def test_a_component_with_no_committed_spelling_is_keyed_by_its_own_id(self):
        got = layout._derive([self._metrics()])
        self.assertEqual(got.size, {CID: 12})
        self.assertEqual(got.edge, {CID: "right"})
        self.assertEqual(got.column, (CID,))
        self.assertEqual(got.fixed_rows, ())
        self.assertEqual(got.variable_rows, frozenset())

    def test_a_built_in_beside_it_still_answers_to_its_committed_spelling(self):
        """The other half of the same line: the fallback must not cost `identity` the
        alias `[frame] slots`, `charter panel top` and every caller in `commands_frame`
        already spell it with."""
        got = layout._derive([
            component.Component(id="identity", title="Identity", edge="top",
                                size=component.Fixed(1), needs=(), events=(),
                                render=lambda ctx: ["id"]),
            self._metrics()])
        self.assertEqual(got.size, {"top": 1, CID: 12})
        self.assertEqual(got.column, (CID,))
        self.assertEqual(got.fixed_rows, ("top",))


class TheFrameHasOneShapeWhateverTheConfigSaid(unittest.TestCase):
    """`frame_of` answers a `components` key on **every** path, the early return
    included.

    That early return is the path very nearly every plane takes, and its answer is what
    `layout._placed_here` reads on every command — `config.FRAME` is this function's
    return value. A key present on one path and absent on another is the
    two-shapes-for-one-answer the comment on that line exists to prevent, and the early
    return is the path nothing had ever asked for the key on.
    """

    def test_a_plane_with_no_usable_frame_section_still_carries_the_key(self):
        for cfg in ({}, {"frame": "nonsense"}, {"frame": []}, {"other": 1},
                    {"frame": None}):
            with self.subTest(cfg=cfg):
                self.assertEqual(instance.frame_of(cfg)["components"], [])

    def test_it_is_the_same_shape_the_configured_path_answers(self):
        """Asked of both paths in one case, so "the shapes agree" is the assertion
        rather than two separate ones that could drift apart."""
        _installed(self)
        placed = instance.frame_of(
            {"frame": {"component": [{"use": CID, "edge": "right", "size": 12}]}})
        self.assertEqual(sorted(instance.frame_of({})), sorted(placed))


class ANameThatIsNotTextIsAnAnswerRatherThanACrash(unittest.TestCase):
    """Three lookups that resolve a component name, each asked something unhashable.

    Every one of them is reached with a value that came out of a committed file — a
    `[frame] slots` list, a `[[frame.component]]` table's `use` — and a committed file
    arrives from someone else's machine (README.md's containment rule). `charter.toml` is
    TOML, so a list or an inline table under a key that wants a string is one keystroke
    away and neither is hashable: a `dict.get` on it raises `TypeError` from inside a
    lookup, half a frame away from the line that was actually wrong, where the refusal
    belongs beside the rest of that value's validation.

    `slots.drawable` is the expensive one — `commands_frame`'s two respawn guards ask it
    before a name reaches tmux CONFIG TEXT, so a `TypeError` there is a launch that dies
    with panes already split.
    """

    #: Unhashable on purpose. A hashable non-string takes the `dict.get` miss path and
    #: answers the same with the guard or without it, so it could not tell one from the
    #: other — it is asked below all the same, because "answers rather than raises" is
    #: the property and it should hold for both kinds.
    _UNHASHABLE = (["top"], {"top": 1}, {"top"}, bytearray(b"top"))
    _HASHABLE = (3, None, b"top", ("top",), True)

    def test_drawable_answers_false(self):
        for value in self._UNHASHABLE + self._HASHABLE:
            with self.subTest(value=value):
                self.assertIs(slots.drawable(value), False)

    def test_component_id_answers_the_value_it_was_given(self):
        for value in self._UNHASHABLE + self._HASHABLE:
            with self.subTest(value=value):
                self.assertIs(builtins.component_id(value), value)

    def test_layouts_key_answers_the_value_it_was_given(self):
        """`layout._key`'s own contract, and stated as that rather than as a consequence
        somewhere else: the three lookups reading it (`_edge_of`, `_size_of`,
        `_is_fixed_row`) each ask a membership question of the result, so an unhashable
        raises at THEIR line whether or not this one filtered it first. What the guard
        decides is which of the two `_key` is — a resolve that answers, or a lookup that
        raises — and `_derive` reads the same `SLOT_OF` in the same direction.
        """
        for value in self._UNHASHABLE + self._HASHABLE:
            with self.subTest(value=value):
                self.assertIs(layout._key(value), value)

    def test_a_hashable_name_nothing_placed_falls_out_of_every_side(self):
        """`_edge_of`'s filter-don't-refuse degrade, which is what makes a name charter
        knows nothing about fall out of `_COLUMN_EDGES`, `_ROW_EDGES` and
        `_BEFORE_EDGES` alike rather than be assigned a side."""
        for value in self._HASHABLE:
            with self.subTest(value=value):
                self.assertIsNone(layout._edge_of(value))
                self.assertIsNone(layout._size_of(value))
                self.assertIs(layout._is_fixed_row(value), False)


class APanelPaintsInsideItsOwnRectangle(PersonaIso, unittest.TestCase):
    """What `_component_text` promises about the pane it paints into, asked with payloads
    that can tell the promises apart.

    The three drawing cases at the top of this file all draw `metrics 42` — eleven
    columns into a forty-column pane, short enough that a width of 40, a width of 80 and
    a width of 1000 all paint the same bytes. So none of them can see the pane's real
    rectangle being used at all.
    """

    def test_a_providers_rows_are_clipped_to_THIS_pane_and_not_to_a_constant(self):
        """§4b property 3 on the one path that actually paints a provider. The width
        `_component_text` builds the ctx with is what `Registry.draw` clips to, so a
        constant there is a line that wraps — and a wrapped line in a pane sized to the
        one row it was supposed to be destroys the frame around it, not only its own
        pane.

        Two pane widths, one narrower than any plausible constant and one wider, because
        a single width can only show that the number is not that one number.
        """
        _installed(self, render="lambda ctx: ['X' * 200]")
        for cols in (40, 100):
            with self.subTest(cols=cols):
                rc, painted, _err = _painted(CID, cols=cols)
                self.assertEqual(rc, 0, painted)
                self.assertEqual([tui.width(line) for line in painted.split("\n")],
                                 [cols])

    def test_a_component_that_declared_nothing_costs_the_tick_no_snapshot(self):
        """§4e's idle cost, and the DECLARATION is what decides it. A provider with an
        empty `needs` is handed an empty ctx, so the plane scan behind `gather.read` is
        never paid for a pane that could not read it anyway — every tick, on every
        frame, for as long as the frame is up.
        """
        _installed(self)
        with mock.patch("charter.frame.gather.read") as read:
            rc, painted, _err = _painted(CID)
        self.assertEqual(rc, 0, painted)
        self.assertIn(_DREW, painted)
        read.assert_not_called()

    def test_a_component_that_declared_a_need_is_handed_the_snapshot(self):
        """The other side of the same branch, so "never reads" cannot be passing for
        "never reads for anybody"."""
        _installed(self, needs=("repos",))
        with mock.patch("charter.frame.gather.read", return_value={}) as read:
            rc, painted, _err = _painted(CID)
        self.assertEqual(rc, 0, painted)
        self.assertIn(_DREW, painted)
        read.assert_called_once_with("f-1")

    def test_a_snapshot_that_cannot_be_read_is_a_line_and_not_a_lost_pane(self):
        """`_component_text`'s own **Never raises**, which is about THIS function's
        failure modes and not the renderer's — a renderer is already contained one layer
        down by `Registry.draw`, which is what
        `test_a_component_that_raises_costs_its_own_pane_and_names_itself` exercises.

        A `gather.read` that fails is transient: the cache directory is on a volume that
        went away, the scan raced an `rm -rf`. Let out, it reaches `run`'s outer handler,
        and `run`'s outer handler `_hold`s the pane — permanently, for a failure that
        would have been over by the next tick.
        """
        _installed(self, needs=("repos",))
        with mock.patch("charter.frame.gather.read",
                        side_effect=OSError("volume went away")):
            rc, painted, err = _painted(CID, cols=78)
        self.assertEqual(rc, 0, painted)
        self.assertIn(CID, painted)
        self.assertIn("OSError", painted)
        self.assertNotIn("panel stopped", err)

    def test_the_failure_line_contains_the_id_before_it_measures_it(self):
        """The order that line is written in, asked in both directions at once.

        *cid* arrived on this process's own command line. Contained FIRST, an escape in
        it becomes four visible characters the width arithmetic then budgets for, and the
        pane names the component that failed. Measured first, `tui.truncate`'s own
        `sanitize` DELETES the escape instead — and the pane names `acme.m`, a component
        that does not exist, which is a confidently wrong answer rather than a contained
        one.

        `_component_text` is asked directly because `run` cannot reach it with such an id
        — `Registry.place` refuses one that is not `_ID_RE`-shaped before a painter is
        ever built, which is `TheProviderNameThatReachesThePaneIsContained` below. The
        containment here is this function's own, on the value this function was handed.
        """
        hostile = "acme.\x1b[2Jm"
        with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((40, 6))):
            line = panel._component_text(builtins.build(), hostile, "f-1")
        self.assertNotIn("\x1b", line)
        self.assertIn("\\x1b", line)
        self.assertNotIn("acme.m", line)
        self.assertLessEqual(tui.width(line), 40)


class TheOperatorsInterruptIsNotAComponentFailure(PersonaIso, unittest.TestCase):
    """A `KeyboardInterrupt` raised while a pane repaints leaves the panel, and
    `_component_text`'s `except Exception` is the whole of what makes it.

    **That function carried an `except KeyboardInterrupt: raise` above the clause, and it
    was dead code** — `tools/sweep.py`'s second find on `main` (#568). Measured against a
    copy of the same function with the clause removed, before removing it: identical on
    `KeyboardInterrupt`, `SystemExit`, `BaseException`, `GeneratorExit`, `OSError`,
    `ValueError`, `RuntimeError`, a `reg.get` miss, and a normal return, whether the
    exception came from the renderer, from `gather.read` or from `_rows`.
    `KeyboardInterrupt` is a `BaseException` and not an `Exception`, so the clause below
    could never have caught one and the clause above could never have been the reason.

    Per the sweep spec's §4 — *"equivalent mutant" and "dead code" are the same finding*
    — it is gone. This class is the property the removal must not change, pinned at the
    step that carries it rather than left to be inferred, the way #566 handled
    `cmd_toggle`'s `if not fid: return 0`.

    **The identical two lines in `Registry.draw` are NOT dead, and that contrast is the
    reason this needed measuring rather than reading.** The clause below THOSE is `except
    BaseException`, which does catch a `KeyboardInterrupt` — so there the guard is the
    only reason an interrupt survives a stranger's renderer at all, and
    `test_component_providers.py::test_the_operators_own_interrupt_still_travels` is what
    says so. Two identical spellings, one load-bearing and one dead, told apart entirely
    by the clause underneath.

    **One axis did differ, and it is reported rather than pinned.** A class inheriting
    from BOTH `KeyboardInterrupt` and `Exception` is legal Python; it escapes
    `Registry.draw` through that live guard and reached the deleted clause first, so
    shipped let it kill the pane where the mutant paints the failure line. Nothing in
    charter or the standard library is such a class — it inverts the split PEP 352 made
    `KeyboardInterrupt` a `BaseException` for — and the behaviour the deleted line
    selected on it was the one §4b exists to refuse: a provider's exception costing the
    SESSION instead of its own pane. Pinning that would have pinned an accident.
    """

    def test_an_interrupt_is_not_an_Exception_which_is_why_the_catch_lets_it_by(self):
        """The chain asserted rather than inferred. Every case below rests on this one
        fact about the hierarchy, so if it ever stopped holding, this says so at the step
        where it changed instead of leaving four cases failing for a reason none of them
        names."""
        self.assertFalse(issubclass(KeyboardInterrupt, Exception))
        self.assertFalse(issubclass(SystemExit, Exception))
        self.assertTrue(issubclass(KeyboardInterrupt, BaseException))
        self.assertTrue(issubclass(SystemExit, BaseException))

    def test_an_interrupt_while_the_snapshot_is_read_leaves_the_panel(self):
        """`gather.read` is `_component_text`'s own failure mode rather than a
        renderer's, so this is the interrupt arriving in the half of the function the
        deleted clause sat over. `^C` during the plane scan must end the process, not
        paint `unavailable` and repaint forever."""
        _installed(self, needs=("repos",))
        with mock.patch("charter.frame.gather.read",
                        side_effect=KeyboardInterrupt("^C")):
            with self.assertRaises(KeyboardInterrupt):
                _painted(CID)

    def test_a_process_exit_while_the_snapshot_is_read_leaves_the_panel_too(self):
        """`run`'s own docstring names both: *"`KeyboardInterrupt` and `SystemExit` are
        how this process is MEANT to end, and swallowing either would hold a pane open
        against the operator killing it."* The deleted clause named only one of them,
        which is a second reason it was not the thing carrying the property."""
        _installed(self, needs=("repos",))
        with mock.patch("charter.frame.gather.read", side_effect=SystemExit(3)):
            with self.assertRaises(SystemExit):
                _painted(CID)

    def test_an_interrupt_inside_a_providers_renderer_leaves_the_panel(self):
        """The whole chain, end to end, through the pane a provider actually draws:
        `Registry.draw`'s live guard re-raises, `_component_text` does not catch it, and
        `run`'s outer `except Exception` does not either. That is three functions
        agreeing, and only the middle one had a line to lose."""
        _installed(self,
                   render="lambda ctx: (_ for _ in ()).throw(KeyboardInterrupt())")
        with self.assertRaises(KeyboardInterrupt):
            _painted(CID)

    def test_an_ordinary_failure_in_the_same_place_is_still_a_painted_line(self):
        """The control, and it is not optional: without it a `_component_text` that had
        stopped catching anything at all would pass every case above. Same call, same
        fixture, an `OSError` instead — contained, named in the pane, and the panel still
        returns 0."""
        _installed(self, needs=("repos",))
        with mock.patch("charter.frame.gather.read",
                        side_effect=OSError("volume went away")):
            rc, painted, err = _painted(CID, cols=78)
        self.assertEqual(rc, 0, painted)
        self.assertIn(CID, painted)
        self.assertIn("OSError", painted)
        self.assertNotIn("panel stopped", err)


class TheProviderNameThatReachesThePaneIsContained(PersonaIso, unittest.TestCase):
    """A component name a stranger's distribution chose, on `run`'s failure path.

    An entry point NAME is not an id charter validated — `builtins.supplies` reads
    distribution metadata and nothing else, so `acme.\\x1b[2Jm` is supplied, and
    `slots.drawable` therefore answers True and `run` goes on to place it. `_stand_in` is
    where it is finally refused, and by then the name is in `run`'s hands and on its way
    to two surfaces: the pane, and stderr.

    **stderr is the one with no other guard.** The pane's line goes through
    `tui.truncate`, whose `sanitize` deletes the escape — leaving `acme.m`, a name that
    is not the one that failed. `print(..., file=sys.stderr)` reaches the operator's
    terminal exactly as written, and an erase-in-display there is an instruction rather
    than a character.
    """

    def test_an_escape_in_a_suppliers_name_reaches_neither_surface_raw(self):
        hostile = "acme.\x1b[2Jm"
        site = _SitePackages(self)
        site.install("hostile-charter", "1.0", {hostile: ENTRY}, {MODULE: _source()})
        self.assertTrue(slots.drawable(hostile), "the fixture never reached `run`")

        rc, painted, err = _painted(hostile)
        self.assertEqual(rc, 1, painted)
        self.assertNotIn("\x1b", err)
        self.assertIn("\\x1b", err)
        self.assertNotIn("acme.m panel stopped", painted)
        self.assertIn("\\x1b", painted)


class TheNameARefusalRepeatsBackIsClipped(PersonaIso, unittest.TestCase):
    """`run`'s OTHER containment call — the unknown-slot refusal, on an unreadable name.

    The twin two lines below it is `TheProviderNameThatReachesThePaneIsContained` above,
    and this line's argument is the same one word for word: **stderr is the surface with
    no other guard.** The pane copy goes through `_hold`, whose `tui.truncate` bounds it
    to the pane whatever it is handed; `print(..., file=sys.stderr)` ships exactly what it
    is given, and `charter panel <name>` is run by hand with stdout redirected often
    enough that `_DEFAULT_ROWS` exists for it.

    **What `contain.one_line` uniquely adds HERE is the clip, not the escaping.** The
    value is already inside a `repr()`, and `repr` escapes ESC, newline, tab, the
    bidirectional overrides and the zero-width joiners on its own — so a hostile name
    reaches stderr contained either way and a case built on an escape would pass with this
    call deleted. What `repr` does not do is stop: it repeats the whole name back, and a
    name is not a value charter chose. `builtins.supplies` reads a stranger's
    `entry_points.txt`, and a 5000-character entry point name is a legal one.

    Asked at two lengths, because a single length can only show that the line is not one
    particular size. The property is that the line does not grow with its input at all —
    which is `DISPLAY_LIMIT`'s own docstring ("a budget a longer input makes longer is not
    a budget"), asked of the one call site that had no case.
    """

    #: The whole sentence after the clipped name. Asserted because a clip that ate the
    #: sentence's own end would leave the operator a name and no list to compare it to,
    #: which is the confidently-truncated answer rather than the contained one.
    _TAIL = f"(known: {', '.join(sorted(slots.SLOTS))})"

    def test_the_refusal_does_not_grow_with_the_name_it_refuses(self):
        lengths = set()
        for chars in (5_000, 20_000):
            with self.subTest(chars=chars):
                rc, painted, err = _painted("acme." + "A" * chars)
                self.assertEqual(rc, 2, painted)
                self.assertIn("acme.AAAA", err,
                              "the refusal stopped naming what it refused")
                self.assertTrue(err.rstrip("\n").endswith(self._TAIL), err[-80:])
                self.assertLess(len(err), 2 * contain.DISPLAY_LIMIT,
                                "a committed value owns the operator's terminal")
                lengths.add(len(err))
        self.assertEqual(len(lengths), 1,
                         f"the line is a function of the name's length: {sorted(lengths)}")


class WhatThisPlanePlacedIsReadBackAndCharterOwnIsNot(unittest.TestCase):
    """`layout._placed_here`, which is how a plane's own rectangle reaches every caller.

    The function is this branch's, and every line of it was unpinned: it is read only by
    `_edge_of` and `_size_of`, both of which consult the shipped tables FIRST — so for the
    four names charter writes, what this returns is never looked at, and for every other
    name charter's own committed frame places nothing at all. Its guards and that ordering
    were protecting each other: delete either one and the suite stayed green, because the
    other made it unobservable. **Two lines that hide each other's absence are not a pin,
    so both are asked here** — that this never offers a built-in, and that `_edge_of` and
    `_size_of` would not take one if it did.

    `config.FRAME` is patched rather than a plane written to disk because this function
    reads that mapping and nothing else, and because two of the four cases below are
    shapes `instance.component_tables` refuses to build — which is the point of asking
    them: this is the second reader of a value the config boundary already validated, and
    a reader that raises on a shape it was not expecting is a `TypeError` from inside a
    lookup, half a frame away from the line that was wrong.
    """

    #: The four built-ins spelled as `[[frame.component]]` tables — the arrangement a
    #: plane writes the moment it wants to add a fifth component and has to name the four
    #: it already had. Resolved through `instance.frame_of` rather than typed out, so the
    #: rectangles are the ones the config boundary actually produces.
    def _four_built_ins(self) -> dict:
        frame = instance.frame_of({"frame": {"component": [
            {"use": "identity"}, {"use": "attention"},
            {"use": "repos"}, {"use": "sidebar"}]}})
        self.assertEqual(frame["slots"], ["top", "bottom", "repos", "right"])
        self.assertEqual(len(frame["components"]), 4, "the fixture placed nothing")
        return frame

    def test_a_built_in_a_plane_spelled_out_is_not_read_back_as_a_plane_placement(self):
        """`Only names the shipped tables do not already carry`, which is the whole of
        what keeps `layout`'s five tables the single answer for charter's own four. A
        plane that spells them out is placing four components with edges and sizes, and
        every one of them would otherwise land in this map beside the table entry it
        duplicates — two answers to "how wide is `right`", which is #500 exactly.
        """
        with mock.patch.dict(config.FRAME, self._four_built_ins()):
            self.assertEqual(layout._placed_here(), {})

    def test_the_shipped_tables_win_over_a_placement_that_claims_a_built_in(self):
        """The other half of that pair, and asked with the guard above stubbed out —
        because with it in place this map can never hold a built-in, so the ordering
        inside `_edge_of` and `_size_of` is unobservable and a case that did not stub it
        would pass against either order.

        `instance.component_tables` refuses a `[[frame.component]]` table that moves or
        resizes one of charter's own, so this arrangement cannot be committed today. That
        is what makes it the right question to ask here: the refusal is at the config
        boundary, and this is the line that means `layout` does not depend on it having
        happened.
        """
        moved = {"right": ("bottom", 3), "repos": ("top", 9), "top": ("right", 40)}
        with mock.patch.object(layout, "_placed_here", return_value=moved):
            self.assertEqual(layout._edge_of("right"), "right")
            self.assertEqual(layout._size_of("right"), layout.SLOT_SIZE["right"])
            self.assertEqual(layout._edge_of("repos"), "bottom")
            self.assertEqual(layout._size_of("repos"), layout.SLOT_SIZE["repos"])
            self.assertIs(layout._is_fixed_row("repos"), False)
            self.assertIs(layout._is_fixed_row("top"), True)

    def test_a_placement_whose_name_is_not_text_costs_only_its_own_entry(self):
        """A `[[frame.component]]` table's `use` arrives from someone else's machine, and
        `charter.toml` is TOML — `use = ["sidebar"]` is one keystroke away and a list is
        not hashable. Without the `isinstance`, the membership test on the next token
        raises `TypeError` from inside `layout`, and it takes the whole arrangement with
        it: the provider placed BESIDE the malformed table never reaches the map, so a
        frame loses a pane it could have drawn over a value it could have skipped.

        The bad tables come first for that reason — under a `_placed_here` with no
        `isinstance` this raises before the good one is ever read.
        """
        components = [{"slot": ["right"], "edge": "right", "size": component.Fixed(9)},
                      {"slot": None, "edge": "bottom", "size": component.Fixed(2)},
                      {"slot": 7, "edge": "top", "size": component.Fixed(1)},
                      {"slot": CID, "edge": "right", "size": component.Fixed(12)}]
        with mock.patch.dict(config.FRAME, {"components": components}):
            self.assertEqual(layout._placed_here(), {CID: ("right", 12)})
            self.assertEqual(layout._edge_of(CID), "right")
            self.assertEqual(layout._size_of(CID), 12)

    def test_a_frame_mapping_that_carries_no_arrangement_is_an_empty_one(self):
        """`instance.frame_of` sets `components` on every path — the early return
        included, which is `TheFrameHasOneShapeWhateverTheConfigSaid` above — so this is
        the same property stated a second time, at the reader instead of at the writer.
        It is stated twice on purpose: this is the only one of `config.FRAME`'s keys read
        with a `.get`, and the reason is that it is the only one that is not a SETTING
        with a default in `instance.FRAME_DEFAULTS`. A key `frame_of` computes is a key a
        later `frame_of` can stop computing, and the cost of that here is every `charter
        frame` command raising `KeyError` on a plane with no `[frame]` section at all.
        """
        for frame, label in (({}, "no components key"), ({"components": None}, "None")):
            with self.subTest(frame=label):
                with mock.patch.dict(config.FRAME, frame, clear=not frame):
                    self.assertEqual(layout._placed_here(), {})
                    self.assertIsNone(layout._edge_of(CID))
                    self.assertEqual(layout._edge_of("top"), "top")


class ASplitIsRefusedRatherThanSizedByANonNumber(unittest.TestCase):
    """`panel_argvs`' last resort, and what taking it out puts on tmux's command line.

    *sizes* is read with a PER-SLOT fallback so a map missing one entry degrades to the
    shipped floor instead of raising inside a launch — and the floor is a `dict`, so it is
    still a `KeyError` for a name nothing placed. The comment on that line calls the
    alternative "the permanently-dead rectangle `_drawable_slots` exists to prevent", and
    with the line deleted the alternative is literal: `size` stays `None`, `str(None)` is
    `"None"`, and `split-window -l None` is what tmux is handed — a launch that has
    already split some of its panes failing on the argument list of the next one.
    """

    _PANELS = dict(session="f-1", socket="testsock", harness_pane="%3")

    def test_a_slot_nothing_can_size_stops_the_plan_rather_than_sizing_it_None(self):
        with self.assertRaises(KeyError):
            layout.panel_argvs(slots=["top", "nope"], sizes={}, **self._PANELS)

    def test_a_sizes_map_missing_one_entry_still_splits_it_at_the_shipped_floor(self):
        """The other side of the same two lines: the fallback exists because it fires,
        and a `KeyError` for `top` would be this function refusing the frame charter
        itself ships."""
        [cmd] = layout.panel_argvs(slots=["top"], sizes={}, **self._PANELS)
        self.assertEqual(cmd[cmd.index("-l") + 1], str(layout.SLOT_SIZE["top"]))


class ASizePolicyBecomesCellsByOneRuleStatedOnce(unittest.TestCase):
    """`_policy_cells` and `_cells` are the same sentence about two spellings of a size.

    `_cells` is handed a COMPONENT and `_policy_cells` a size POLICY, which is why there
    are two of them: `_placed_here` reads the resolved `[[frame.component]]` table, and a
    placement carries the policy without the component. What they must never be is two
    RULES — the number this one returns is the number that reaches `split-window -l` for a
    component this plane placed, and the number `_cells` returns is the one in
    `layout.SLOT_SIZE` for the components charter wrote, and a frame in which those two
    disagree is a stack sized in two different units.

    So they are asked together rather than separately, and across every policy
    `frame/component.py` defines. Only `Fixed` is reachable through
    `instance.component_tables` today, which refuses a `[[frame.component]]` table whose
    `size` is not a number — that is exactly why the non-`Fixed` half needs a case here
    rather than none: it is the half nothing else on this machine can reach, and without
    it `_policy_cells` is not the rule `_cells` states but only the first line of it.
    """

    def test_a_policy_and_a_component_carrying_it_answer_the_same_number(self):
        for size in (component.Fixed(1), component.Fixed(22), component.Content(),
                     component.Content(cap=9), component.Fill()):
            with self.subTest(size=size):
                whole = component.Component(
                    id=CID, title="Metrics", edge="right", size=size,
                    needs=(), events=(), render=lambda ctx: [_DREW])
                self.assertEqual(layout._policy_cells(size), layout._cells(whole))

    def test_a_pane_whose_height_is_its_content_is_still_worth_one_cell(self):
        """The floor itself, stated as a number rather than only as an agreement: a
        `Content` or `Fill` pane has no answer to give before anything has measured it,
        and `component.cells` refuses a size below one cell because a panel nobody can
        see is a panel nobody asked for."""
        for size in (component.Content(), component.Content(cap=9), component.Fill()):
            with self.subTest(size=size):
                self.assertEqual(layout._policy_cells(size), 1)


class TheCommittedSpellingAndTheComponentIdReachOneEntry(unittest.TestCase):
    """`layout._key`'s alias resolution — this file's own title, read in the direction
    nothing was asking it in.

    `_derive` keys all five tables by the COMMITTED spelling, so every per-slot fact
    `layout` answers is reachable under a built-in's component id only because `_key`
    resolves one to the other. **Collapsing `_key` to `return name` left the full suite
    green.** `tools/sweep.py` found that on `main`, after three rounds of hand-sweeping
    had walked past it (#568).

    The one case that named `_key` asked it for a value it must hand back unchanged
    (`ANameThatIsNotTextIsAnAnswerRatherThanACrash` above). That is the guard's OTHER
    half, and it answers identically with the resolution or without it — a real test,
    pointed at the function, blind to the thing the line is for, which is the shape this
    file's docstring says the review rounds kept hitting.

    So the resolution is asked here on every alias the table declares rather than on one
    of them, and then at the three geometry functions a wrong answer actually reaches. A
    sidebar that stops costing columns is #500; a strip that stops being a fixed row is
    #515. Neither arrives as an error — `slot_sizes` and `_edge_of` both degrade by
    filtering rather than refusing, which is exactly why nothing went red.
    """

    #: Every alias :data:`builtins.SLOT_OF` declares, written out rather than read back
    #: from it: a test that reads the table it is checking moves with the table and pins
    #: nothing. :meth:`test_the_pinned_table_is_the_whole_table` is what keeps the two in
    #: step instead.
    #:
    #: **`repos` is here because it is its OWN alias.** It is the one entry a `_key` that
    #: resolved nothing would still get right, so it is the reason the other three have to
    #: be asked beside it — one example standing for four would have a one-in-four chance
    #: of being the example that cannot fail.
    ALIASES = (("identity", "top"), ("attention", "bottom"),
               ("repos", "repos"), ("sidebar", "right"))

    def test_the_pinned_table_is_the_whole_table(self):
        """An alias added to `SLOT_OF` and not to :data:`ALIASES` would ship unpinned in
        exactly the way `identity` did, so the coverage is asserted rather than
        believed."""
        self.assertEqual(dict(self.ALIASES), builtins.SLOT_OF)

    def test_a_built_in_id_resolves_to_the_slot_name_the_tables_are_keyed_by(self):
        """`_key`'s own contract, stated as the mapping and not as an example of it."""
        for cid, slot in self.ALIASES:
            with self.subTest(cid=cid):
                self.assertEqual(layout._key(cid), slot)

    def test_every_per_slot_fact_answers_the_same_under_either_spelling(self):
        """The three lookups that read `_key`, asked in both vocabularies at once.

        `assertIsNotNone` on the slot-name side first, because `_edge_of` and `_size_of`
        answer `None` for a name nothing placed: without that control a resolution that
        had stopped working would agree with itself at `None` and pass.
        """
        for cid, slot in self.ALIASES:
            with self.subTest(cid=cid):
                self.assertIsNotNone(layout._edge_of(slot), "the fixture placed nothing")
                self.assertIsNotNone(layout._size_of(slot), "the fixture placed nothing")
                self.assertEqual(layout._edge_of(cid), layout._edge_of(slot))
                self.assertEqual(layout._size_of(cid), layout._size_of(slot))
                self.assertIs(layout._is_fixed_row(cid), layout._is_fixed_row(slot))

    def test_a_sidebar_under_its_component_id_still_costs_the_table_its_columns(self):
        """`repos_cols`, against the measurement in its own docstring: on tmux 3.7c at
        120x40 a side pane split BEFORE the table leaves that table 97 columns, not the
        window's 120. The sidebar reaches that arithmetic through `_edge_of` and
        `_size_of`, so an arrangement spelled in component ids would be sized for a pane
        23 columns wider than the one it gets — #500 exactly, whose visible form was a
        seven-row pane with one line in it.
        """
        wanted = 120 - layout.SLOT_SIZE["right"] - layout._BORDER_COLS
        self.assertEqual(
            layout.repos_cols(["right", "top", "bottom", "repos"], window_cols=120),
            wanted)
        self.assertEqual(
            layout.repos_cols(["sidebar", "identity", "attention", "repos"],
                              window_cols=120),
            wanted)

    def test_a_frame_spelled_in_component_ids_is_sized_as_the_slot_names_are(self):
        """`slot_sizes` drops a name it cannot size rather than raising on it, matching
        `visible_slots`' filter-don't-refuse discipline — so an unresolved id is not an
        error here, it is an ABSENT entry, and `panel_argvs` then splits that pane at the
        shipped floor or the frame loses it. Asked as "the two spellings agree" so the
        numbers stay `_derive`'s rather than being copied into this file.
        """
        ids = [cid for cid, _ in self.ALIASES]
        by_id = layout.slot_sizes(ids, window_rows=50, content_rows=6)
        by_name = layout.slot_sizes([slot for _, slot in self.ALIASES],
                                    window_rows=50, content_rows=6)
        self.assertEqual(len(by_name), len(self.ALIASES), "the fixture sized nothing")
        self.assertEqual({builtins.SLOT_OF[cid]: n for cid, n in by_id.items()}, by_name)

    def test_a_sidebar_under_its_component_id_is_still_charged_no_rows(self):
        """`harness_rows` charges a pane's height against the harness unless its edge
        costs COLUMNS, and that edge comes from `_edge_of`. Unresolved, `sidebar`'s 22
        columns are charged as 22 rows and the harness loses them — the same arithmetic
        `test_the_harness_is_charged_no_rows_for_a_pane_that_costs_columns` pins for a
        component this plane placed, asked here for charter's own sidebar under its own
        id. The number that comes out of this is what `resize-pane -y` is given, so it is
        wrong on screen and not only in a dict.
        """
        by_name = {"top": 1, "bottom": 1, "repos": 6, "right": 22}
        by_id = {"identity": 1, "attention": 1, "repos": 6, "sidebar": 22}
        rows = sum(by_name[s] + layout._BORDER_ROWS for s in ("top", "bottom", "repos"))
        self.assertEqual(layout.harness_rows(by_name, window_rows=50), 50 - rows)
        self.assertEqual(layout.harness_rows(by_id, window_rows=50), 50 - rows)
