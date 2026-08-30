"""What a provider is handed so it can match, and what charter does not promise it —
Phase 4 of `docs/superpowers/specs/2026-08-28-frame-visual-design.md`.

§7's sentence, made a test: **charter owns the surface and the border; the provider owns
every cell it writes.** The surface is underneath, painted by tmux into the pane's whole
rectangle before a provider draws anything, so a component that paints nothing gets it for
free. What charter hands over on top of that is the recipes — `ctx.chrome` — and nothing
else: it does not overdraw a provider's heading, it does not take a row out of §4b's
rectangle to make things line up, and it does not promise that a pane looks like charter's.

**The counter-argument §7 answers is the one this file is really about.** A provider that
paints its own colours next to charter's can make a pane unreadable, and charter cannot
prevent that — a provider's module is ordinary Python and `ctx`'s own docstring says
outright that it is not a sandbox. What charter *can* guarantee is the three things it
already guarantees, and they are asserted here rather than restated: the provider's paint
stops at its rectangle, its failure costs its pane and not the session, and its output is
contained before it reaches the terminal.

**And the ROUND TRIP, which is the assertion this file was missing** (#707). Two halves
were each pinned here — that the recipes are served, and that a foreign row is
contained — and nothing spanned them, so from the day #604 landed charter handed a provider
``ctx.chrome`` and
then escaped it back into the literal text ``\\x1b[1mMetrics\\x1b[0m`` on the way to the
pane, with every test green. `TheRecipeSurvivesToThePane` below installs a real
distribution whose renderer writes the recipes and asserts what arrives in the pane,
because a property that spans two mechanisms cannot be asserted at either end of it.
"""

from __future__ import annotations

import os
import re
import sys
import unittest
from types import MappingProxyType
from unittest import mock

from charter import contain, statusline, tui
from charter.frame import chrome, component, ctx, registry, slots
from tests._isolation import PersonaIso
from tests.test_component_providers import CID, ENTRY, MODULE, _SitePackages, _source

#: One SGR escape with its parameter list captured — the shape `chrome._SGR` matches.
_SGR = re.compile(r"\x1b\[([0-9;]*)m")

#: SGR parameters that introduce a colour charter did not get from the operator's own
#: palette: `38`/`48` followed by `5` (a 256-cube index) or `2` (a 24-bit triple).
_EXTENDED = ("38", "48")


def _absolute_colours(text: str) -> list[str]:
    """Every escape in *text* that names a colour charter picked rather than a slot.

    Asked of the PARAMETERS, never by searching for the substring `48;5;`: `\\x1b[1;38;5;
    236m` is a cube index with a `1;` in front of it and a substring test walks past it.
    That is this project's own recurring defect — the spelling standing in for the
    property — and this file is one of the places it would be least visible.
    """
    found = []
    for m in _SGR.finditer(text):
        params = m.group(1).split(";")
        for i, p in enumerate(params[:-1]):
            if p in _EXTENDED and params[i + 1] in ("5", "2"):
                found.append(m.group(0))
    return found


def _tty(value: bool = True):
    """`sys.stdout.isatty()` answering *value* — what `chrome.colour_ok` asks."""
    return mock.patch.object(sys.stdout, "isatty", return_value=value, create=True)


class TheRecipesAreDataAndNothingElse(unittest.TestCase):
    """4.1. A `MappingProxyType` of strings, no callable, reading nothing."""

    def setUp(self):
        self.env = mock.patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.tty = _tty()
        self.tty.start()
        self.addCleanup(self.tty.stop)

    def test_it_is_a_read_only_mapping(self):
        r = chrome.recipes()
        self.assertIsInstance(r, MappingProxyType)
        with self.assertRaises(TypeError):
            r["ok"] = "\x1b[35m"
        with self.assertRaises(TypeError):
            del r["ok"]

    def test_the_underlying_dict_is_not_reachable_through_it(self):
        """A `MappingProxyType` over a dict a caller still holds is a read-only view of a
        mutable thing. Each call builds its own, so one component cannot edit what the
        next one is about to be handed."""
        a, b = chrome.recipes(), chrome.recipes()
        self.assertEqual(dict(a), dict(b))
        self.assertIsNot(a, b)

    def test_every_value_is_a_string_and_no_value_is_callable(self):
        for role, value in chrome.recipes().items():
            with self.subTest(role=role):
                self.assertIsInstance(value, str)
                self.assertFalse(callable(value))

    def test_the_vocabulary_is_the_roles_a_renderer_can_write(self):
        self.assertEqual(set(chrome.recipes()),
                         {"heading", "muted", "selected", "ok", "warn", "bad", "reset",
                          "inset"})

    def test_surface_and_focus_are_absent_rather_than_empty(self):
        """§5's other two elements are tmux pane options. A recipe answering `''` for
        them would claim to hand over something charter does not have — the convincing
        empty `FRAME_CHROME` refuses an `auto` value for."""
        for role in ("surface", "focus"):
            with self.subTest(role=role):
                self.assertNotIn(role, chrome.recipes())


class TheRecipesNameNoColourCharterChose(unittest.TestCase):
    """The rule `instance.FRAME_CHROME` keeps, reaching the one place a stranger's code
    would otherwise have to guess it."""

    def setUp(self):
        self.env = mock.patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.tty = _tty()
        self.tty.start()
        self.addCleanup(self.tty.stop)

    def test_no_recipe_carries_a_cube_index_or_a_24_bit_triple(self):
        for role, value in chrome.recipes().items():
            with self.subTest(role=role):
                self.assertEqual(_absolute_colours(value), [])

    def test_the_check_recognises_both_forms(self):
        """The control. Every assertion above is a negative, so this proves the check can
        fail — including the prefixed spelling a grep for `\\x1b[48;5;` misses."""
        for esc in ("\x1b[48;5;236m", "\x1b[38;2;30;60;90m", "\x1b[1;38;5;236m"):
            with self.subTest(esc=esc):
                self.assertEqual(_absolute_colours(esc), [esc])

    def test_the_status_roles_are_the_three_names_the_spec_fixed(self):
        """§5.6: `ok` → green, `warn` → yellow, `bad` → red, never an index. Written down
        so the next role added does not reach for `colour208`."""
        r = chrome.recipes()
        self.assertEqual(r["ok"], statusline._GREEN)
        self.assertEqual(r["warn"], statusline._YELLOW)
        self.assertEqual(r["bad"], statusline._RED)


class ARecipeIsTheSameStringCharterUses(PersonaIso, unittest.TestCase):
    """The exit criterion: a component drawn with the recipes is indistinguishable from a
    built-in at the same size.

    Asserted as identity of the ESCAPES rather than by rendering two panes and comparing
    pictures: what "matches" means here is that the provider's heading is the weight
    charter's heading is, and a second copy of `\\033[1m` in `chrome.py` is exactly how the
    two would drift apart while both tests stayed green.
    """

    def setUp(self):
        super().setUp()
        self.env = mock.patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.tty = _tty()
        self.tty.start()
        self.addCleanup(self.tty.stop)

    def test_the_heading_recipe_is_the_weight_charters_own_heading_uses(self):
        r = chrome.recipes()
        head = slots._sidebar_head("personas", 6, 40)
        self.assertIn(f"{r['heading']}personas", head)

    def test_the_muted_recipe_is_the_weight_charters_own_count_uses(self):
        r = chrome.recipes()
        head = slots._sidebar_head("personas", 6, 40)
        self.assertIn(f"{r['muted']} 6", head)

    def test_the_selected_recipe_is_what_chrome_reverse_asserts(self):
        """One constant, not two: `chrome.reverse` re-asserts this escape after every SGR
        that cancels it, and a provider highlighting its own row has to be able to write
        the same one."""
        self.assertTrue(chrome.reverse("x", 4).startswith(chrome.recipes()["selected"]))

    def test_the_reset_recipe_closes_a_span_the_way_charter_closes_one(self):
        self.assertEqual(chrome.recipes()["reset"], tui.RESET)

    def test_the_inset_recipe_is_exactly_the_column_content_starts_at(self):
        """§5.4 is a COLUMN, not an attribute — the one entry here that is not SGR. Served
        as the literal left edge so a provider prepends it and lines up, rather than being
        told a number and left to spell the padding, which is the per-call-site spelling
        that constant exists to end."""
        r = chrome.recipes()
        self.assertEqual(tui.width(r["inset"]), slots.INSET)
        self.assertEqual(r["inset"], slots._inset())


class NoColourReachesTheRecipesToo(unittest.TestCase):
    """§3.2's rule, and the half a provider would otherwise break for charter.

    `NO_COLOR` means no colour on the operator's screen caused by charter, whichever
    process puts the bytes there. A provider handed live escapes under `NO_COLOR` would
    emit them on charter's behalf.
    """

    def test_every_sgr_role_is_empty_under_no_color(self):
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}, clear=True), _tty():
            r = chrome.recipes()
        for role in ("heading", "muted", "selected", "ok", "warn", "bad", "reset"):
            with self.subTest(role=role):
                self.assertEqual(r[role], "")

    def test_presence_and_not_a_value(self):
        """`NO_COLOR=` and `NO_COLOR=0` both count, per no-color.org. Matching a value
        would be the spelling-not-property mistake in the file that is about it."""
        for value in ("", "0", "1", "false"):
            with self.subTest(value=value):
                with mock.patch.dict(os.environ, {"NO_COLOR": value}, clear=True), _tty():
                    self.assertEqual(chrome.recipes()["ok"], "")

    def test_a_stdout_that_is_not_a_tty_is_the_same_answer(self):
        with mock.patch.dict(os.environ, {}, clear=True), _tty(False):
            self.assertEqual(chrome.recipes()["heading"], "")

    def test_the_roles_are_still_all_there(self):
        """Empty rather than ABSENT, and the difference is a provider's pane. A component
        writing `ctx.chrome["ok"] + text` would raise inside its own draw and lose its
        rectangle — to honour a request about colour."""
        with mock.patch.dict(os.environ, {}, clear=True), _tty():
            live = set(chrome.recipes())
        with mock.patch.dict(os.environ, {"NO_COLOR": "1"}, clear=True), _tty():
            self.assertEqual(set(chrome.recipes()), live)

    def test_the_inset_survives_because_it_is_not_colour(self):
        """A frame that lost its inset under `NO_COLOR` would be answering a question
        about colour with a change to layout."""
        with mock.patch.dict(os.environ, {"NO_COLOR": "1"}, clear=True), _tty():
            self.assertEqual(tui.width(chrome.recipes()["inset"]), slots.INSET)

    def test_the_word_chrome_is_on_changes_none_of_them(self):
        """§7 asks for the roles "resolved for this frame's `chrome` setting", and the
        measurement says there is nothing for that setting to resolve: `off`/`dark`/
        `light` select a pane background, `window-style` honours colour and silently
        ignores every attribute (re-measured for Phase 3 on tmux 3.7c AND 3.2 — `bold`,
        `dim` and `reverse` each put no SGR at all on an attached client's wire), and no
        renderer can write a pane option.

        So this is a NEGATIVE pinned deliberately: a `chrome` parameter on `recipes()`
        would be an argument that cannot change an answer. If a future role does depend on
        the word, this test is what has to be deleted, with the measurement that justified
        it."""
        import inspect
        self.assertEqual(list(inspect.signature(chrome.recipes).parameters), [])


class TheRecipesTravelOnTheCtx(PersonaIso, unittest.TestCase):
    """4.1/4.2. `ctx.chrome`, served whatever the component declared."""

    def _ctx(self, needs=()):
        return ctx.build(needs, width=80, height=10, fid="f1", snapshot={})

    def test_a_component_that_declared_nothing_still_gets_the_recipes(self):
        self.assertIn("chrome", vars(self._ctx()))

    def test_it_is_served_alongside_the_geometry_and_not_out_of_the_snapshot(self):
        """`SERVES`' values are functions OF THE SNAPSHOT. A recipes entry there would be
        a callable that takes the snapshot and ignores it, which is the exact shape
        `ctx.Ctx`'s docstring records as the defect that let an action reach the plane's
        whole vault inventory."""
        self.assertIn("chrome", ctx.ALWAYS)
        self.assertNotIn("chrome", ctx.SERVES)

    def test_the_refusal_message_names_it_among_what_charter_serves(self):
        c = self._ctx()
        with self.assertRaises(AttributeError) as e:
            c.nothing_like_this
        self.assertIn("chrome", str(e.exception))

    def test_it_cannot_be_replaced_on_the_ctx(self):
        c = self._ctx()
        with self.assertRaises(AttributeError):
            c.chrome = {"ok": "\x1b[35m"}

    def test_the_ctx_still_carries_no_callable_of_charters(self):
        """4.6's exit criterion, re-asserted because this phase added a field: `dir(ctx)`
        and `vars(ctx)` carry data and no way to *do* anything."""
        c = self._ctx(("gather",))
        for name in dir(c):
            if name.startswith("_"):
                continue
            with self.subTest(name=name):
                self.assertFalse(callable(getattr(c, name)))

    def test_two_components_in_one_repaint_cannot_edit_each_others_recipes(self):
        a, b = self._ctx(), self._ctx()
        self.assertIsNot(a.chrome, b.chrome)
        with self.assertRaises(TypeError):
            a.chrome["heading"] = "x"
        self.assertEqual(b.chrome["heading"], chrome.recipes()["heading"])


class AProviderThatIgnoresTheRecipesHarmsNothingOutsideItsPane(unittest.TestCase):
    """4.3. The three guarantees §7 says charter *can* make, asserted rather than restated.

    A provider whose rows carry a background and no reset is the case that matters, because
    it is the one this whole spec makes normal: components are about to start painting.

    **Colour is LIVE for every case here, and that is the harder half rather than the
    tidier one.** `chrome.contain_row` escapes every escape when `colour_ok()` is false
    (§3.2), so a case that let the suite's own piped stdout decide would be asserting
    containment with the containment's own reason turned off — and would have gone on
    passing if the vocabulary hole #707 opened had been cut the whole width of SGR. With a
    tty and no `NO_COLOR`, `\\x1b[41m` is escaped because 41 is not a parameter charter
    serves, which is the property.
    """

    def setUp(self):
        self.env = mock.patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.tty = _tty()
        self.tty.start()
        self.addCleanup(self.tty.stop)

    def test_a_foreign_row_is_clipped_to_its_rectangle(self):
        """`Registry._fit` contains and clips every foreign row. A row wider than the pane
        cannot reach the pane beside it, whatever it painted."""
        wide = "\x1b[41m" + "X" * 200
        got = registry._fit([wide], width=20, height=3, escape=True)
        for line in got:
            with self.subTest(line=line):
                self.assertLessEqual(tui.width(line), 20)

    def test_a_foreign_rows_escapes_are_neutralised_before_they_reach_the_terminal(self):
        """`escape=True` is what `Registry.draw` passes for a component id in
        `_foreign`. The paint a provider emits is data by the time it is measured."""
        got = registry._fit(["\x1b[41mLEAK"], width=20, height=1, escape=True)
        self.assertNotIn("\x1b[41m", "".join(got))

    def test_a_foreign_component_cannot_add_rows_to_its_pane(self):
        """The height budget is charter's. A provider that returned fifty rows for a
        three-row pane cannot push the pane below it down the screen."""
        got = registry._fit(["r"] * 50, width=20, height=3, escape=True)
        self.assertLessEqual(len(got), 3)

    def test_a_leaked_background_costs_one_paint_and_not_the_session(self):
        """Phase 1.5, re-asserted from the provider's side because §7 promises it to a
        provider author. `panel._write` prefixes a reset, so `\\x1b[2J` — which erases
        with whatever attributes are set — cannot carry a component's leaked background
        into every later paint of that pane."""
        from charter.frame import panel
        wrote = []
        # A tty and no `NO_COLOR`: the reset is SGR, so under the other answer `_write`
        # correctly emits none of it — including this — and a fixture that did not say so
        # would be asserting the absence of a guard it had itself turned off.
        with mock.patch.dict(os.environ, {}, clear=True), _tty(), \
             mock.patch.object(panel, "_rows", return_value=5), \
             mock.patch.object(panel.sys.stdout, "write", side_effect=wrote.append), \
             mock.patch.object(panel.sys.stdout, "flush"):
            panel._write("\x1b[41mLEAK")
        out = "".join(wrote)
        self.assertTrue(out.startswith("\x1b[m"),
                        f"the clear-screen was not preceded by a reset: {out!r}")
        self.assertLess(out.index("\x1b[m"), out.index("\x1b[2J"))
        self.assertIn("\x1b[41mLEAK", out,
                      "the provider's own paint was supposed to survive into its own "
                      "pane — the reset is about the NEXT paint, not this one")

    def test_the_split_four_call_sites_rely_on_still_answers_the_content(self):
        """The reset goes BEFORE the cursor-home for this reason and it is asserted here
        rather than left to the four tests that would break: `split("\x1b[2J", 1)[1]`
        must still be the content."""
        from charter.frame import panel
        wrote = []
        with mock.patch.dict(os.environ, {}, clear=True), _tty(), \
             mock.patch.object(panel, "_rows", return_value=5), \
             mock.patch.object(panel.sys.stdout, "write", side_effect=wrote.append), \
             mock.patch.object(panel.sys.stdout, "flush"):
            panel._write("BODY")
        self.assertEqual("".join(wrote).split("\x1b[2J", 1)[1], "BODY")


class TheRecipeSurvivesToThePane(unittest.TestCase):
    """#707, and the assertion that would have caught it: the ROUND TRIP.

    Everything above this class asserts one END of the channel — `chrome.recipes` serves
    the escapes, `registry._fit` contains a foreign row — and both stayed green for every
    day a provider's ``ctx.chrome['heading']`` arrived in the pane as the six visible
    characters of its own escape. Neither half was wrong on its own; the property that
    spans them was never asked for.

    **So this asks for it end to end, through a real installed distribution.** The module
    under `sys.path` is written by `_SitePackages` exactly as
    `tests.test_component_providers` writes one, its renderer reads `ctx.chrome` the way
    `docs/frame.md`'s worked example tells a provider to, and the assertion is the tuple
    `Registry.draw` answers — which is what `frame/panel.py` joins and writes into the
    pane. A fixture that registered the component directly and reached into `_foreign`
    would be asserting charter's plumbing rather than the seam a provider actually
    arrives through.
    """

    def setUp(self):
        self.site = _SitePackages(self)
        self.env = mock.patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.tty = _tty()
        self.tty.start()
        self.addCleanup(self.tty.stop)

    def _drew(self, render: str) -> tuple[str, ...]:
        """The rows a real installed provider whose renderer is *render* put in its pane.

        A registry per call, not one per case. `Registry.place` answers what is already
        registered rather than re-importing — a component keeps the rectangle it went onto
        the frame in — so a case that varies the renderer across subtests would draw the
        FIRST one seven times and pass on six roles it never exercised.
        """
        self.site.install("acme-charter", "1.4.0", {CID: ENTRY},
                          {MODULE: _source(render=render)})
        r = registry.Registry()
        r.place(CID)
        return r.draw(CID, ctx.build((), width=60, height=4, fid="fr-1", snapshot={}))

    def test_the_worked_example_in_the_docs_reaches_the_pane_as_colour(self):
        """`docs/frame.md`'s own provider, verbatim in shape: inset, heading, reset, muted.

        The literal text `\\x1b[1mMetrics\\x1b[0m` is what this used to answer, so the
        negative is asserted beside the positive — a row that is neither is a third
        failure, and only naming both tells them apart.
        """
        drew = self._drew(
            "lambda ctx: [ctx.chrome['inset'] + ctx.chrome['heading'] + 'Metrics' "
            "+ ctx.chrome['reset'] + ctx.chrome['muted'] + ' 12' + ctx.chrome['reset']]")
        served = chrome.recipes()
        self.assertEqual(
            drew,
            (f"{served['inset']}{served['heading']}Metrics{served['reset']}"
             f"{served['muted']} 12{served['reset']}",))
        self.assertNotIn("\\x1b", "".join(drew))

    def test_every_role_charter_serves_survives_the_trip(self):
        """One case per role rather than one case for `heading`. A vocabulary is a set,
        and a hole in it is exactly the shape of a role that was added to `recipes` and
        not to what the containment admits."""
        served = chrome.recipes()
        for role in ("heading", "muted", "selected", "ok", "warn", "bad", "reset"):
            with self.subTest(role=role):
                drew = self._drew(f"lambda ctx: [ctx.chrome[{role!r}] + 'x']")
                self.assertEqual(drew, (f"{served[role]}x",))

    def test_the_provider_row_is_the_row_charters_own_component_would_have_drawn(self):
        """§7's exit criterion, and the reason the round trip is the assertion: a
        component drawn with the recipes is INDISTINGUISHABLE from a built-in at the same
        size. The same renderer is run once as a provider and once as charter's own, and
        the two tuples must be equal — which they were not, because only one of them was
        contained."""
        drew = self._drew("lambda ctx: [ctx.chrome['heading'] + 'Metrics' "
                          "+ ctx.chrome['reset']]")
        own = registry.Registry()
        own.register(component.Component(
            id="builtin.metrics", title="Metrics", edge="right",
            size=component.Fixed(12), needs=(),
            render=lambda c: [c.chrome["heading"] + "Metrics" + c.chrome["reset"]]))
        self.assertEqual(
            drew,
            own.draw("builtin.metrics",
                     ctx.build((), width=60, height=4, fid="fr-1", snapshot={})))

    def test_a_provider_that_hard_codes_an_escape_charter_does_not_serve_is_contained(self):
        """The other side of the same trip, asserted here rather than only at `_fit`: the
        hole is the size of the vocabulary and no larger. A cube index is a colour charter
        never picked, so it arrives as the characters of its own escape."""
        drew = self._drew(r"lambda ctx: ['\x1b[38;5;236mcube']")
        self.assertEqual(drew, ("\\x1b[38;5;236mcube",))

    def test_under_no_color_the_provider_emits_nothing_and_carries_nothing(self):
        """Both halves of §3.2 at once. `recipes` serves the empty string, so a component
        built on it writes no escape — and a component that hard-coded one has it escaped,
        because charter passing an SGR through on a provider's behalf is charter asking
        somebody else to paint."""
        with mock.patch.dict(os.environ, {"NO_COLOR": "1"}):
            drew = self._drew(
                r"lambda ctx: [ctx.chrome['heading'] + 'served', '\x1b[1mhard']")
        self.assertEqual(drew, ("served", "\\x1b[1mhard"))


class TheContainmentAdmitsCharterSVocabularyAndNothingElse(unittest.TestCase):
    """What `chrome.contain_row` lets through, asked of the SGR parameters as numbers.

    **Every case here is a row a provider could legally return**, and the split is the
    one #707 argued: an escape that costs zero columns and names an attribute or a slot in
    the operator's own palette cannot reach the pane beside it or forge a colour charter
    did not pick, and everything else can. Matching the recipes' SPELLING would put the
    first three kept cases on the wrong side — which is this repo's own recurring defect,
    in the file that is about it.
    """

    def setUp(self):
        self.env = mock.patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.tty = _tty()
        self.tty.start()
        self.addCleanup(self.tty.stop)

    def _fit(self, row: str) -> str:
        return registry._fit([row], width=120, height=1, escape=True)[0]

    def test_a_spelling_charter_does_not_write_is_still_the_vocabulary(self):
        """`\\x1b[01m` is bold with a leading zero and `\\x1b[m` is a reset with no
        parameter at all. Neither is a string `chrome._role_values` contains, and both do
        exactly what a recipe does."""
        for row in ("\x1b[01mbold", "\x1b[mreset", "\x1b[0;1mboth"):
            with self.subTest(row=row):
                self.assertEqual(self._fit(row), row)

    def test_two_roles_in_one_escape_are_two_parameters_and_not_a_new_colour(self):
        """A provider composing `heading` and `ok` into one escape wrote `\\x1b[1;32m`,
        which charter never writes and which says only what charter serves."""
        self.assertEqual(self._fit("\x1b[1;32mgreen heading"), "\x1b[1;32mgreen heading")

    def test_a_colour_outside_the_operators_palette_does_not_survive(self):
        """The three ways to name a colour charter did not get from the sixteen, each
        arriving as the characters of its own escape."""
        for row, seen in (("\x1b[38;5;236mcube", "\\x1b[38;5;236mcube"),
                          ("\x1b[38;2;30;60;90mtrue", "\\x1b[38;2;30;60;90mtrue"),
                          ("\x1b[41mbackground", "\\x1b[41mbackground")):
            with self.subTest(row=row):
                self.assertEqual(self._fit(row), seen)

    def test_one_served_parameter_beside_one_that_is_not_carries_neither(self):
        """The arm a per-parameter check needs and a per-escape one misses: `\\x1b[1;41m`
        is bold AND a background charter never picked, and half an escape is not a thing
        that can be kept."""
        self.assertEqual(self._fit("\x1b[1;41mmixed"), "\\x1b[1;41mmixed")

    def test_nothing_that_moves_the_cursor_or_talks_to_the_terminal_survives(self):
        """The whole reason the containment exists, and it is untouched: these are what a
        row would use to draw over the pane beside it, and none of them is an SGR."""
        for row in ("\x1b[2Jerase", "\x1b[10Aup", "\x1b]0;title\x07osc", "\x1bcreset",
                    "a\x07bell", "two\nlines", "tab\there"):
            with self.subTest(row=row):
                self.assertNotIn("\x1b", self._fit(row))
                self.assertNotIn("\x07", self._fit(row))

    def test_charters_own_rows_are_still_not_contained_at_all(self):
        """`escape` is provenance. A built-in's markup goes through untouched, including
        the colours charter picked for its own panes, which containing would corrupt while
        protecting nothing."""
        row = "\x1b[38;5;236mcharter's own\x1b[0m"
        self.assertEqual(registry._fit([row], width=120, height=1, escape=False)[0], row)

    def test_with_colour_off_the_vocabulary_is_empty(self):
        """§3.2 is a statement about what charter emits, not about what it serves — so it
        has to reach the containment as well as the recipes."""
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}):
            self.assertEqual(self._fit("\x1b[1mbold"), "\\x1b[1mbold")

    def test_a_pane_that_is_not_a_terminal_is_the_same_answer(self):
        with _tty(False):
            self.assertEqual(self._fit("\x1b[1mbold"), "\\x1b[1mbold")


class TheVocabularyIsTheRolesAndNotASecondListOfThem(unittest.TestCase):
    """`chrome.served_params` is derived from `chrome._role_values`, and #707 is what a
    second list costs: a role served at one end and unknown at the other."""

    def setUp(self):
        self.env = mock.patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.tty = _tty()
        self.tty.start()
        self.addCleanup(self.tty.stop)

    def test_every_role_charter_serves_is_admitted_by_the_containment(self):
        """The property the two halves have to share, asked directly. A role that stopped
        being admitted would fail here as well as in the round trip above — one of them is
        the seam and this one is the reason."""
        for role, value in chrome._role_values().items():
            with self.subTest(role=role):
                served = chrome.served_params()
                for m in _SGR.finditer(value):
                    self.assertTrue(chrome.is_recipe(m.group(1), served), value)

    def test_a_role_added_upstream_widens_the_vocabulary_without_a_second_edit(self):
        """The control, and the whole argument for deriving it. A magenta role appears in
        `_role_values` alone and the containment admits it on the same breath — so the
        drift #707 was cannot happen by somebody forgetting the other list."""
        self.assertFalse(chrome.is_recipe("35", chrome.served_params()))
        with mock.patch.object(chrome, "_role_values",
                               return_value={"brand": "\x1b[35m"}):
            self.assertTrue(chrome.is_recipe("35", chrome.served_params()))

    def test_the_vocabulary_is_the_seven_roles_and_stops_there(self):
        """Pinned as a set rather than left implicit: widening it is a widening of what a
        stranger's code may put on an operator's screen, and it should cost a test change
        and the conversation that goes with it."""
        self.assertEqual(sorted(chrome.served_params()), [0, 1, 2, 7, 31, 32, 33])

    def test_the_inset_is_not_in_it_because_it_is_not_an_escape(self):
        """The one recipe that is spaces. It survives containment the way every other
        space does, which is why it is not in the vocabulary and does not need to be."""
        self.assertNotIn("\x1b", chrome.recipes()["inset"])


class TheCharacterBudgetSurvivesTheHoleThatWasCutInIt(unittest.TestCase):
    """`registry.LINE_LIMIT` bounds CHARACTERS, and `tui.truncate`'s width clamp cannot:
    a combining mark measures zero cells. Opening the vocabulary put escapes on the
    keeping side of that budget, and both of `contain_row`'s breaks are what that costs.
    """

    def setUp(self):
        self.env = mock.patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.tty = _tty()
        self.tty.start()
        self.addCleanup(self.tty.stop)

    def test_a_row_of_invisibles_is_still_bounded(self):
        """The bound the width clamp cannot give: a million combining marks fit any pane
        and cost the repaint a million characters."""
        got = chrome.contain_row("́" * 5000, limit=64)
        self.assertLessEqual(len(got), 65)

    def test_a_row_whose_budget_runs_out_mid_way_keeps_none_of_the_tail(self):
        row = "\n" * 20 + "\x1b[1m" + "TAIL"
        got = chrome.contain_row(row, limit=16)
        self.assertNotIn("TAIL", got)
        self.assertLessEqual(len(got), 17)

    def test_an_unbounded_parameter_list_is_in_the_vocabulary_and_still_refused(self):
        """The `keep` break. An SGR's parameter list has no length limit, so
        `\\x1b[` + `1;` half a million times + `m` is every parameter charter serves and is
        no colour anyone will ever see. Taken whole or not at all — a slice of it is a
        bare ESC, which is the one thing this must never manufacture."""
        row = "\x1b[" + "1;" * 400 + "1m" + "after"
        self.assertTrue(chrome.is_recipe(row[2:row.index("m")], chrome.served_params()))
        got = chrome.contain_row(row, limit=64)
        self.assertNotIn("\x1b", got)
        self.assertLessEqual(len(got), 65)

    def test_a_recipe_that_lands_exactly_on_the_budget_is_still_inside_it(self):
        """**The boundary, and not only the direction** — the deletion sweep asked for
        this one by name. `used + len(text) > limit` and `>= limit` differ on exactly one
        row: an escape whose last character lands ON the limit. It is inside the budget,
        and reading it as outside would drop a recipe from every row that happened to end
        there, which is a vocabulary that narrows by one escape with no rule behind it.
        The row one character longer is the control, so the case pins where the line is
        rather than only that there is one.
        """
        self.assertEqual(chrome.contain_row("\x1b[1m", limit=4), "\x1b[1m")
        self.assertEqual(chrome.contain_row("\x1b[1m", limit=3), "")

    def test_a_recipe_that_fits_the_budget_is_kept_whole(self):
        """The control for the break above: the same shape under the limit comes through,
        so the refusal is about the budget and not about the parameter list."""
        row = "\x1b[1;1;1m" + "after"
        self.assertEqual(chrome.contain_row(row, limit=64), row)

    def test_the_limit_reaches_the_containment_from_the_registry(self):
        """`registry._fit` is the only caller in production and it passes `LINE_LIMIT`.
        Asserted rather than assumed, because a default spelled here as well would be the
        second, weaker copy this repo deletes."""
        long_row = "́" * (registry.LINE_LIMIT * 2)
        self.assertLessEqual(len(registry._fit([long_row], width=10, height=1,
                                               escape=True)[0]),
                             registry.LINE_LIMIT + 1)


class TheBudgetNeedsNoSecondGuard(unittest.TestCase):
    """The property an `if used >= limit: break` was written for, pinned without it.

    That guard was there so `contain.one_line` could not be handed a NEGATIVE limit —
    which it reads as a slice from the end of the string, answering with the tail of what
    it was hiding. It could not happen: `chrome._pieces` strictly alternates escaped and
    kept pieces, and a kept piece is appended only when it FITS, so every escaped piece
    starts from a `used` that is at most the limit. The guard was deleted and this is what
    keeps the reasoning honest — the deletion sweep's own shape, and the same one
    `chrome.fill` and `slots.inset_rows` already carry.
    """

    #: Rows are built from these, so a case is a real mixture of what a provider returns:
    #: two recipes, two colours outside the vocabulary, an unbounded parameter list, the
    #: whitespace controls `one_line` expands four-to-one, and ordinary text.
    #:
    #: **The last two are what `chrome._escaped`'s fast path is judged on.**
    #: `str.isprintable()` is false for every *Other* and *Separator* category, and
    #: `contain.one_line` rewrites a narrower set — its `_INVISIBLE` five plus whitespace
    #: that is not an ASCII space. U+00A0 is `Zs` and IS rewritten; U+E000 is `Co` and is
    #: NOT. Both fall off the fast path, and both have to come back exactly what `one_line`
    #: answers, which is what makes the equivalence case a test of the shortcut rather than
    #: of the common case it shortcuts.
    _ALPHABET = ("\x1b[1m", "\x1b[0m", "\x1b[41m", "\x1b[38;5;9m", "\x1b[1;1;1;1;1;1m",
                 "\n", "\t", "\x07", "a", "́", "\xa0", "\ue000")

    def setUp(self):
        # `colour_ok` answering a plain `True`, and NOT `_tty()` like every other class
        # here. These cases make about 27 000 calls, and a `Mock` records every one of
        # them: measured, the same fixture through `mock.patch.object` spends 2.2s where
        # this spends a fifth of it, and the recorded calls are a list that only grows.
        # What the cases are about is the budget, and the answer this pins is the one
        # `NoColourReachesTheRecipesToo` and the containment class already assert for real.
        self.env = mock.patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.live = mock.patch.object(chrome, "colour_ok", lambda: True)
        self.live.start()
        self.addCleanup(self.live.stop)

    def _rows(self):
        """Every row of up to three pieces of :data:`_ALPHABET` — exhaustive, not sampled.

        1 111 rows, so the property is *proved* over the shape rather than sampled from
        it. A random fuzz would make a failure depend on a seed, and a containment defect
        that reproduces once a week is a defect nobody can bisect.

        Three is where the alternation is complete rather than an arbitrary stopping
        point: a row of three already yields five pieces — escaped, kept, escaped, kept,
        escaped — which is every position `contain_row`'s loop can be in. A fourth adds
        rows and no new shape, at ten times the cost on a suite that runs on every push.
        """
        from itertools import product
        for n in range(4):
            for parts in product(self._ALPHABET, repeat=n):
                yield "".join(parts)

    def test_one_line_is_never_handed_a_negative_limit(self):
        limits = []
        real = contain.one_line

        def spy(value, limit=contain.DISPLAY_LIMIT):
            limits.append(limit)
            return real(value, limit)

        with mock.patch.object(chrome.contain, "one_line", side_effect=spy):
            for row in self._rows():
                for limit in (1, 3, 8, 64):
                    chrome.contain_row(row, limit=limit)
        self.assertTrue(limits, "the spy saw nothing — the fixture stopped testing")
        self.assertEqual([x for x in limits if x < 0], [])

    def test_the_answer_is_never_longer_than_the_budget_and_its_ellipsis(self):
        """The bound the deleted guard was also credited with. One clipped piece is the
        most a row can have — after it, the next piece is a kept escape that no longer
        fits — so `limit + 1` is the whole of it."""
        for row in self._rows():
            for limit in (1, 3, 8, 64):
                got = chrome.contain_row(row, limit=limit)
                if len(got) > limit + 1:
                    self.fail(f"{row!r} at {limit} came back {got!r}")

    def test_no_answer_ever_carries_half_an_escape(self):
        """The one thing an escape-aware containment must never manufacture. A bare ESC in
        a pane is an introducer for whatever character follows it."""
        for row in self._rows():
            for limit in (1, 3, 8, 64):
                got = chrome.contain_row(row, limit=limit)
                if "\x1b" in _SGR.sub("", got):
                    self.fail(f"{row!r} at {limit} came back {got!r}")

    def test_a_row_with_no_recipe_in_it_is_contained_exactly_as_it_always_was(self):
        """**The strongest statement this change can make**: for a row carrying none of
        charter's own vocabulary, `chrome.contain_row` answers `contain.one_line`'s own
        string, character for character. The containment did not become cleverer or
        weaker — it grew a hole exactly the size of the roles charter publishes, and
        anything outside that hole is treated by the same function it always was.

        It is also what pins `chrome._escaped`'s `isprintable()` fast path, which exists
        because this is on the repaint path: a piece that skips `one_line`'s per-character
        loop has to come out identical, and both shapes are in the alphabet above.
        """
        served = chrome.served_params()
        checked = 0
        for row in self._rows():
            if any(chrome.is_recipe(m.group(1), served) for m in _SGR.finditer(row)):
                continue
            checked += 1
            for limit in (1, 3, 8, 64):
                got, was = (chrome.contain_row(row, limit=limit),
                            contain.one_line(row, limit=limit))
                if got != was:
                    self.fail(f"{row!r} at {limit}: {got!r} is no longer {was!r}")
        self.assertGreater(checked, 100,
                           "the filter took every row — this case stopped testing")


if __name__ == "__main__":
    unittest.main()
