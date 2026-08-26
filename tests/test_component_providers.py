"""The seam a stranger's code arrives through: discovery, refusal, isolation, containment.

**Every distribution below is real.** `importlib.metadata` is not stubbed anywhere in this
file — each case writes a real `.dist-info` with a real `entry_points.txt` and a real
importable module into a directory it puts on ``sys.path``, which is exactly what an
installed package is as far as `importlib.metadata` is concerned. A stubbed
`entry_points` would prove the stub works: it would answer with whatever shape the test
imagined, and the day charter reads `ep.dist.version` or a duplicate name off the real API
the test would keep passing while the frame refused every provider on the machine.

The four properties §4b says must survive a stranger's code, plus the fifth from its
closing paragraph, each with a case that fails without the guard:

1. **A mismatched `API_VERSION` does not load**, and the refusal names the provider, the
   version it speaks and the version charter speaks (§4g). Checked before the provider
   builds anything, which is asserted rather than assumed.
2. **An id two providers claim loads NEITHER**, names both, and the rest of the frame is
   drawn (§4h).
3. **A provider that raises costs its own pane** — on import, while building, or in
   ``render`` — and the pane NAMES it. A blank pane is the confidently-wrong output the
   left sidebar was retired for, and #512's lesson twice over: a convincing empty is worse
   than a refusal.
4. **Charter contains what came back**, after the component returned it, with
   `contain.one_line` before the width arithmetic and `tui.width` rather than `len`.
5. **A missing provider is a message**, in the rectangle config asked for, and the rest of
   the frame is drawn.

**Nothing here reads the machine.** Each case asserts about ids its own fixture installed,
never about the set of providers this machine happens to carry, so a suite run on a
developer's laptop with a real provider installed gives the answer CI gives.
"""

from __future__ import annotations

import importlib
import shutil
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

from charter import tui
from charter.frame import component, ctx, registry

CID = "acme.metrics"
MODULE = "acme_metrics"
ENTRY = f"{MODULE}:metrics"


#: "say nothing about the version and let the fixture pick charter's own" — an object
#: rather than ``None``, because ``None`` is one of the values a case below installs AS
#: the declared version, and a sentinel that is also a fixture value is a test that
#: quietly stops testing.
_CHARTERS = object()


def _source(*, cid: str = CID, api: object = _CHARTERS,
            render: str = "lambda ctx: ['ok']", head: str = "") -> str:
    """A provider module: a version, a factory, and a record of whether it ran.

    ``built`` is what makes "refused before the provider built anything" an assertion
    rather than a hope — the module object survives the refusal in ``sys.modules``, so a
    test can ask it afterwards.
    """
    api = component.API_VERSION if api is _CHARTERS else api
    return textwrap.dedent(f"""\
        from charter.frame import component
        {head}
        API_VERSION = {api!r}
        built = False


        def metrics():
            global built
            built = True
            return component.Component(
                id={cid!r}, title="Metrics", edge="right",
                size=component.Fixed(12), needs=(), events=(),
                render={render})
        """)


class _SitePackages:
    """A directory that is an installed environment, as `importlib.metadata` sees one.

    One directory holding every distribution a case installs, because that is what
    site-packages is: two providers claiming one entry point name are two `.dist-info`
    directories side by side, which is the only way to build §4h's collision without
    inventing a shape for it.

    Cleanups are bound methods of THIS object rather than of the test, so nothing here
    can be replaced by a subclass defining a method of the same name — the shape that
    silently dropped a base class's cleanup once already.
    """

    def __init__(self, case: unittest.TestCase) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-provider-"))
        self._modules: list[str] = []
        case.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        case.addCleanup(self._unpath)
        case.addCleanup(self._forget)
        sys.path.insert(0, str(self.root))
        importlib.invalidate_caches()

    def install(self, dist: str, version: str, entries: dict[str, str],
                modules: dict[str, str] | None = None) -> None:
        """Write one distribution: its metadata, its entry points, its modules.

        Installing over a distribution of the same name REPLACES it, module included —
        a second copy left in ``sys.modules`` would serve the previous case's renderer to
        the next one, and a second `.dist-info` beside it would look to charter exactly
        like §4h's collision. Both are how a fixture quietly stops testing.
        """
        for name, src in (modules or {}).items():
            (self.root / f"{name}.py").write_text(src, encoding="utf-8")
            sys.modules.pop(name, None)
            self._modules.append(name)
        info = self.root / f"{dist.replace('-', '_')}-{version}.dist-info"
        info.mkdir(exist_ok=True)
        (info / "METADATA").write_text(
            f"Metadata-Version: 2.1\nName: {dist}\nVersion: {version}\n", encoding="utf-8")
        (info / "entry_points.txt").write_text(
            "[charter.components]\n" + "".join(f"{k} = {v}\n" for k, v in entries.items()),
            encoding="utf-8")
        importlib.invalidate_caches()

    def imported(self, name: str) -> bool:
        """Whether *name* has actually been imported into this interpreter."""
        return name in sys.modules

    def _forget(self) -> None:
        for name in self._modules:
            sys.modules.pop(name, None)

    def _unpath(self) -> None:
        while str(self.root) in sys.path:
            sys.path.remove(str(self.root))
        importlib.invalidate_caches()


def _ctx(width: int = 40, height: int = 8):
    return ctx.build((), width=width, height=height, fid="fr-1", snapshot={})


def _builtin(cid: str = "repos", edge: str = "bottom"):
    """A component charter registered itself — the rest of the frame, in one object."""
    return component.Component(id=cid, title=cid.title(), edge=edge,
                               size=component.Content(), needs=(), events=(),
                               render=lambda c: [f"{cid} drew"])


class DiscoveryIsRealAndLazy(unittest.TestCase):
    """An installed provider is FOUND from metadata and IMPORTED only when placed."""

    def setUp(self):
        self.site = _SitePackages(self)
        self.r = registry.Registry()

    def test_an_installed_provider_is_listed(self):
        self.site.install("acme-charter", "1.4.0", {CID: ENTRY},
                          {MODULE: _source()})
        self.assertIn(CID, registry.Providers().ids())

    def test_listing_does_not_import_the_provider(self):
        """The whole cost argument: an installed-but-unplaced provider costs nothing.

        The evidence is a module that CANNOT be imported without saying so — listing it
        would raise here rather than quietly succeed, so this cannot pass by the import
        happening to be cheap.
        """
        self.site.install("boom-charter", "2.0", {"boom.metrics": "boom_metrics:metrics"},
                          {"boom_metrics": "raise RuntimeError('imported at discovery')\n"})
        providers = registry.Providers()
        self.assertIn("boom.metrics", providers.ids())
        self.assertTrue(providers.supplies("boom.metrics"))
        self.assertFalse(self.site.imported("boom_metrics"))

    def test_placing_it_imports_it_and_draws_it(self):
        self.site.install("acme-charter", "1.4.0", {CID: ENTRY},
                          {MODULE: _source(render="lambda ctx: ['42 open', '3 stale']")})
        placed = self.r.place(CID)
        self.assertTrue(self.site.imported(MODULE))
        self.assertEqual(placed.id, CID)
        self.assertEqual(self.r.get(CID).title, "Metrics")
        self.assertEqual([c.id for c in self.r.on_edge("right")], [CID])
        self.assertEqual(self.r.draw(CID, _ctx()), ("42 open", "3 stale"))
        self.assertEqual(self.r.failures, {})

    def test_a_provider_may_name_the_component_itself_rather_than_a_factory(self):
        """`acme_charter.metrics:Component` is the spec's own example spelling."""
        self.site.install("acme-charter", "1.4.0", {CID: f"{MODULE}:widget"},
                          {MODULE: _source() + "\nwidget = metrics()\n"})
        self.assertEqual(self.r.place(CID).id, CID)

    def test_placing_the_same_component_twice_answers_the_one_that_is_placed(self):
        self.site.install("acme-charter", "1.4.0", {CID: ENTRY}, {MODULE: _source()})
        first = self.r.place(CID)
        self.assertIs(self.r.place(CID), first)


class TheApiVersionIsRefusedAtLoad(unittest.TestCase):
    """§4g: one integer, refused at load. Not semver negotiation, not a shim."""

    def setUp(self):
        self.site = _SitePackages(self)
        self.r = registry.Registry()
        self.other = component.API_VERSION + 6

    def _install(self, api):
        self.site.install("acme-charter", "9.9.9", {CID: ENTRY},
                          {MODULE: _source(api=api)})

    def test_a_provider_speaking_another_version_does_not_load(self):
        self._install(self.other)
        with self.assertRaises(component.ComponentError) as e:
            registry.Providers().load(CID)
        said = str(e.exception)
        self.assertIn("acme-charter 9.9.9", said)
        self.assertIn(f"version {self.other}", said)
        self.assertIn(f"charter speaks {component.API_VERSION}", said)

    def test_the_component_it_wanted_to_supply_is_not_on_the_frame(self):
        """The half that matters: refusing is not enough if it is drawn anyway."""
        self._install(self.other)
        self.r.place(CID)
        self.assertEqual(self.r.get(CID).title, f"{CID} — not drawn")
        self.assertIn("acme-charter 9.9.9", self.r.failures[CID])

    def test_the_version_is_read_before_the_provider_builds_anything(self):
        """Refused at LOAD means before its code runs, not after it has built a panel."""
        self._install(self.other)
        self.r.place(CID)
        self.assertFalse(sys.modules[MODULE].built)

    def test_a_provider_declaring_no_version_at_all_does_not_load(self):
        self.site.install("acme-charter", "9.9.9", {CID: ENTRY},
                          {MODULE: _source().replace(f"API_VERSION = {component.API_VERSION!r}",
                                                     "")})
        with self.assertRaises(component.ComponentError) as e:
            registry.Providers().load(CID)
        self.assertIn("API_VERSION", str(e.exception))
        self.assertIn("acme-charter 9.9.9", str(e.exception))

    def test_a_version_that_is_not_an_integer_does_not_load(self):
        """``"1"`` and ``True`` are the two a permissive check lets through: ``True ==
        1`` in Python, so a bare equality would load a provider declaring a boolean."""
        for api in ("1", True, 1.0, None):
            with self.subTest(api=api):
                self._install(api)
                with self.assertRaises(component.ComponentError) as e:
                    registry.Providers().load(CID)
                self.assertIn("API_VERSION", str(e.exception))


class OneIdIsClaimedByOneProvider(unittest.TestCase):
    """§4h: silently picking one means a pane whose origin cannot be determined."""

    def setUp(self):
        self.site = _SitePackages(self)
        self.r = registry.Registry()
        self.site.install("acme-charter", "1.4.0", {CID: ENTRY}, {MODULE: _source()})
        self.site.install("other-charter", "0.2", {CID: "other_metrics:metrics"},
                          {"other_metrics": _source()})

    def test_a_collision_loads_neither_and_names_both(self):
        with self.assertRaises(component.ComponentError) as e:
            registry.Providers().load(CID)
        said = str(e.exception)
        self.assertIn("acme-charter 1.4.0", said)
        self.assertIn("other-charter 0.2", said)
        self.assertIn(CID, said)

    def test_neither_provider_is_even_imported(self):
        """The collision is visible in metadata, so no stranger's code needs to run."""
        self.r.place(CID)
        self.assertFalse(self.site.imported(MODULE))
        self.assertFalse(self.site.imported("other_metrics"))

    def test_the_rest_of_the_frame_is_drawn(self):
        self.r.register(_builtin())
        self.r.place(CID)
        self.assertEqual([c.id for c in self.r.on_edge("bottom")], ["repos", CID])
        self.assertEqual(self.r.draw("repos", _ctx()), ("repos drew",))

    def test_the_pane_says_which_two(self):
        self.r.place(CID)
        drawn = " ".join(self.r.draw(CID, _ctx(width=60)))
        self.assertIn("acme-charter", drawn)
        self.assertIn("other-charter", drawn)


class TheEntryPointNameIsTheId(unittest.TestCase):
    """What config places must be what charter can resolve without importing anything."""

    def setUp(self):
        self.site = _SitePackages(self)
        self.r = registry.Registry()

    def test_a_provider_answering_a_different_id_does_not_load(self):
        self.site.install("acme-charter", "1.4.0", {CID: ENTRY},
                          {MODULE: _source(cid="repos")})
        with self.assertRaises(component.ComponentError) as e:
            registry.Providers().load(CID)
        self.assertIn(CID, str(e.exception))
        self.assertIn("repos", str(e.exception))

    def test_it_cannot_take_over_a_built_in_that_way(self):
        """The reason the check exists: a config naming one thing loading another."""
        self.r.register(_builtin())
        self.site.install("acme-charter", "1.4.0", {CID: ENTRY},
                          {MODULE: _source(cid="repos")})
        self.r.place(CID)
        self.assertEqual(self.r.draw("repos", _ctx()), ("repos drew",))

    def test_a_bare_id_nothing_registered_is_a_message_not_a_lookup(self):
        """A typo'd built-in and a provider id spelled without its namespace, at once."""
        self.r.place("repose")
        self.assertIn("repose", self.r.failures)
        self.assertIn("namespaced", self.r.failures["repose"])

    def test_an_id_that_could_never_name_a_component_is_refused(self):
        with self.assertRaises(component.ComponentError):
            self.r.place("acme\nmetrics")


class AMissingProviderIsAMessage(unittest.TestCase):
    """§4b: a committed config must never make charter unusable for somebody else."""

    def setUp(self):
        self.site = _SitePackages(self)          # installed: nothing
        self.r = registry.Registry()

    def test_the_component_config_named_becomes_a_pane_that_says_so(self):
        self.r.place(CID)
        drawn = " ".join(self.r.draw(CID, _ctx(width=70)))
        self.assertIn(CID, drawn)
        self.assertIn("no installed provider", drawn)

    def test_the_rest_of_the_frame_is_drawn(self):
        self.r.register(_builtin())
        self.r.register(_builtin("identity", edge="top"))
        self.r.place(CID)
        self.assertEqual(self.r.draw("repos", _ctx()), ("repos drew",))
        self.assertEqual(self.r.draw("identity", _ctx()), ("identity drew",))

    def test_the_standin_keeps_the_rectangle_config_asked_for(self):
        """So a machine missing one provider draws the same frame, minus one pane's text.

        Order is geometry — ``["top","bottom","right"]`` gives a 200-column bottom row and
        ``["top","right","bottom"]`` gives 177 — so a standin that took a different edge
        would move every panel registered after it.
        """
        self.r.register(_builtin("identity", edge="top"))
        placed = self.r.place(CID, edge="right", size=component.Fixed(12))
        self.assertEqual(placed.edge, "right")
        self.assertEqual(placed.size, component.Fixed(12))
        self.assertEqual(self.r.split_order("identity"), 1)
        self.assertEqual(self.r.split_order(CID), 2)

    def test_it_is_a_pane_and_not_a_hole(self):
        """A panel that is simply absent is the frame lying about the plane (#512)."""
        self.r.place(CID, edge="right", size=component.Fixed(12))
        self.assertEqual([c.id for c in self.r.on_edge("right")], [CID])


class AProviderThatRaisesCostsItsOwnPane(unittest.TestCase):
    """§4b property 1, on every path a stranger's code can fail on."""

    def setUp(self):
        self.site = _SitePackages(self)
        self.r = registry.Registry()
        self.r.register(_builtin())

    def _install(self, module_src, entry=ENTRY):
        self.site.install("acme-charter", "1.4.0", {CID: entry}, {MODULE: module_src})

    def _still_alive(self):
        self.assertEqual(self.r.draw("repos", _ctx()), ("repos drew",))

    def test_a_module_that_raises_on_import_costs_its_pane(self):
        self._install("raise RuntimeError('no such database')\n")
        self.r.place(CID)
        drawn = " ".join(self.r.draw(CID, _ctx(width=70)))
        self.assertIn(CID, drawn)
        self.assertIn("RuntimeError", drawn)
        self.assertIn("no such database", drawn)
        self._still_alive()

    def test_a_factory_that_raises_costs_its_pane(self):
        self._install(f"API_VERSION = {component.API_VERSION!r}\n\n\n"
                      "def metrics():\n"
                      "    raise ValueError('bad config')\n")
        self.r.place(CID)
        drawn = " ".join(self.r.draw(CID, _ctx(width=70)))
        self.assertIn("ValueError", drawn)
        self.assertIn("bad config", drawn)
        self._still_alive()

    def test_an_entry_point_naming_nothing_costs_its_pane(self):
        self._install(_source(), entry=f"{MODULE}:absent")
        self.r.place(CID)
        self.assertIn("absent", " ".join(self.r.draw(CID, _ctx(width=70))))
        self._still_alive()

    def test_a_render_that_raises_names_the_component_and_the_failure(self):
        self._install(_source(render="lambda ctx: 1 / 0"))
        self.r.place(CID)
        drawn = " ".join(self.r.draw(CID, _ctx(width=70)))
        self.assertIn(CID, drawn)
        self.assertIn("ZeroDivisionError", drawn)
        self._still_alive()

    def test_a_render_that_raises_never_draws_a_blank_pane(self):
        """The specific defect: nothing at all, in a frame that looks deliberate."""
        self._install(_source(render="lambda ctx: 1 / 0"))
        self.r.place(CID)
        self.assertNotEqual(self.r.draw(CID, _ctx()), ())

    def test_a_render_that_exits_the_process_costs_its_pane(self):
        """`SystemExit` is not an `Exception`, and it would take the session with it."""
        self._install(_source(render="lambda ctx: sys.exit(3)", head="import sys"))
        self.r.place(CID)
        self.assertIn("SystemExit", " ".join(self.r.draw(CID, _ctx(width=70))))
        self._still_alive()

    def test_the_operators_own_interrupt_still_travels(self):
        """Containment is not a sandbox, and a frame that cannot be stopped is worse."""
        self._install(_source(render="lambda ctx: (_ for _ in ()).throw(KeyboardInterrupt())"))
        self.r.place(CID)
        with self.assertRaises(KeyboardInterrupt):
            self.r.draw(CID, _ctx())

    def test_a_render_answering_something_that_is_not_lines_costs_its_pane(self):
        for answer, wanted in (("'one long string'", "one long string"),
                               ("None", "None"),
                               ("{'rows': []}", "rows"),
                               ("[None]", "None"),
                               ("[b'bytes']", "bytes")):
            with self.subTest(answer=answer):
                self._install(_source(render=f"lambda ctx: {answer}"))
                r = registry.Registry()
                r.place(CID)
                drawn = " ".join(r.draw(CID, _ctx(width=70)))
                self.assertIn(CID, drawn)
                self.assertIn(wanted, drawn)
                self._still_alive()


class CharterContainsWhatCameBack(unittest.TestCase):
    """§4b property 3: applied by charter AFTER the component returned, not trusted to it."""

    def setUp(self):
        self.site = _SitePackages(self)
        self.r = registry.Registry()

    def _draw(self, render, *, width=20, height=4):
        self.site.install("acme-charter", "1.4.0", {CID: ENTRY},
                          {MODULE: _source(render=render)})
        self.r.place(CID)
        return self.r.draw(CID, _ctx(width=width, height=height))

    def test_an_escape_sequence_is_shown_rather_than_obeyed(self):
        """A row of ESC would move the cursor out of its own rectangle and draw over the
        pane beside it. The component renders committed values — a branch name is one —
        so this is a value crossing into a format with structure, again.

        **Asserted as the escape being VISIBLE, not merely absent.** `tui.truncate`
        deletes a non-markup escape on its way past, so "no ESC in the row" is a property
        a DIFFERENT guard already provides and this test would pass with charter's
        containment removed entirely — the shape this suite has shipped before. What
        `contain.one_line` adds is that the row says what was in it, which is the same
        argument `contain.py` makes for five kinds of "no answer": a defect folded behind
        a silent deletion is a defect nobody fixes.
        """
        rows = self._draw(r"lambda ctx: ['\x1b[2Jwiped']", width=40)
        self.assertNotIn("\x1b", rows[0])
        self.assertEqual(rows[0], r"\x1b[2Jwiped")

    def test_a_newline_cannot_forge_a_second_row(self):
        """Shown as ``\\x0a`` for the same reason: `tui.truncate` alone turns it into a
        space, and a row silently joined is one nobody knows was two."""
        rows = self._draw(r"lambda ctx: ['one\ntwo']", width=40)
        self.assertEqual(rows, (r"one\x0atwo",))

    def test_a_row_is_clipped_to_the_pane_in_CELLS_and_not_in_characters(self):
        """Width, not length: 20 CJK characters are 40 columns and would overflow the
        pane by exactly the amount `len` cannot see."""
        rows = self._draw("lambda ctx: ['世' * 40]", width=20)
        self.assertLessEqual(tui.width(rows[0]), 20)
        self.assertLess(len(rows[0]), 20)

    def test_more_rows_than_the_pane_has_are_dropped(self):
        rows = self._draw("lambda ctx: ['row %d' % i for i in range(100)]", height=4)
        self.assertEqual(rows, ("row 0", "row 1", "row 2", "row 3"))

    def test_a_pane_with_no_room_draws_nothing(self):
        self.site.install("acme-charter", "1.4.0", {CID: ENTRY},
                          {MODULE: _source(render="lambda ctx: ['anything']")})
        self.r.place(CID)
        self.assertEqual(self.r.draw(CID, _ctx(width=0, height=0)), ())

    def test_a_row_of_zero_width_characters_is_still_bounded(self):
        """`tui.truncate` bounds CELLS, and a combining mark is none of them — so a row
        that measures nothing can still be a megabyte. Both bounds, or neither works."""
        rows = self._draw("lambda ctx: ['́' * 200000]", width=20)
        self.assertLessEqual(len(rows[0]), registry.LINE_LIMIT + 1)

    def test_charters_own_markup_survives_its_own_renderer(self):
        """The other half of the rule, and why escaping is provenance and not a flag.

        Charter's renderers contain their committed values where they interpolate them
        and add charter's colour on top; escaping their output here would corrupt
        charter's own markup while protecting nothing.
        """
        bold = "\x1b[1m"
        self.r.register(component.Component(
            id="identity", title="Identity", edge="top", size=component.Fixed(1),
            needs=(), events=(), render=lambda c: [f"{bold}charter{tui.RESET}"]))
        self.assertEqual(self.r.draw("identity", _ctx()),
                         (f"{bold}charter{tui.RESET}",))
