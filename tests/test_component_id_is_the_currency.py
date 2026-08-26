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

from charter import commands_frame, config, instance
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


def _installed(case, *, render: str = f"lambda ctx: [{_DREW!r}]", head: str = "") -> None:
    """Put one real provider distribution supplying :data:`CID` on ``sys.path``."""
    site = _SitePackages(case)
    site.install("acme-charter", "1.0", {CID: ENTRY},
                 {MODULE: _source(render=render, head=head)})
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


class CharterOwnConfigIsUnchanged(unittest.TestCase):
    """charter's own committed `charter.toml` resolves to the frame it always drew.

    #535 removed the repo table from charter's own plane and a reviewer caught it, not a
    test. This reads the file off disk and pins the whole resolved arrangement — the slot
    list, the component ids behind it, and every rectangle — so a refactor that speaks a
    new vocabulary has to keep answering in the old one.
    """

    def setUp(self):
        self.cfg = tomllib.loads(_COMMITTED.read_text(encoding="utf-8"))

    def test_the_slot_list_is_the_one_that_is_committed(self):
        self.assertEqual(instance.frame_of(self.cfg)["slots"],
                         ["top", "bottom", "repos", "right"])

    def test_it_declares_no_arrangement_so_nothing_extra_is_placed(self):
        """`slots` is the shorthand this plane is written in, so there are no
        `[[frame.component]]` tables and `layout` has no per-plane rectangle to read."""
        self.assertIsNone(instance.component_tables(self.cfg.get("frame")))
        self.assertEqual(instance.frame_of(self.cfg)["components"], [])

    def test_every_placement_is_the_built_in_it_always_was(self):
        got = instance.frame_components(self.cfg)
        self.assertEqual([(p["use"], p["slot"], p["edge"], p["visible"]) for p in got],
                         [("identity", "top", "top", True),
                          ("attention", "bottom", "bottom", True),
                          ("repos", "repos", "bottom", True),
                          ("sidebar", "right", "right", True)])

    def test_the_frame_it_splits_is_byte_for_byte_the_frame_it_split(self):
        """The whole launch argv, not a summary of it. `slot_sizes` and `panel_argvs`
        are where a re-keyed table would show up first, and they are asserted against
        literals rather than against the tables under test."""
        f = instance.frame_of(self.cfg)
        with mock.patch.dict(config.FRAME, f):
            sizes = layout.slot_sizes(f["slots"], window_rows=50, content_rows=6)
            self.assertEqual(sizes, {"top": 1, "bottom": 1, "repos": 6, "right": 22})
            argvs = layout.panel_argvs(slots=f["slots"], session="f-1", socket="/sock",
                                       harness_pane="%0", sizes=sizes)
        self.assertEqual([a[a.index("split-window") + 1:] for a in argvs], [
            ["-t", "%0", "-v", "-b", "-l", "1", "-P", "-F", "#{pane_id}", "--",
             *layout.panel_command(slot="top", session="f-1")],
            ["-t", "%0", "-v", "-l", "1", "-P", "-F", "#{pane_id}", "--",
             *layout.panel_command(slot="bottom", session="f-1")],
            ["-t", "%0", "-v", "-l", "6", "-P", "-F", "#{pane_id}", "--",
             *layout.panel_command(slot="repos", session="f-1")],
            ["-t", "%0", "-h", "-l", "22", "-P", "-F", "#{pane_id}", "--",
             *layout.panel_command(slot="right", session="f-1")],
        ])
