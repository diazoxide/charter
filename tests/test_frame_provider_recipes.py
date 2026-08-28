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
"""

from __future__ import annotations

import os
import re
import sys
import unittest
from types import MappingProxyType
from unittest import mock

from charter import statusline, tui
from charter.frame import chrome, ctx, registry, slots
from tests._isolation import PersonaIso

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
        with mock.patch.dict(os.environ, {"NO_COLOR": "1"}, clear=True), _tty():
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
    """

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


if __name__ == "__main__":
    unittest.main()
