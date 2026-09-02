"""Per-pane style: a background and an inset each component asks for by itself.

The report this answers, verbatim: *"i think will be good to make background colors of
panes configurable / on sidebar and repo-list pane add padding / so each pane can have
custom background color from charter.toml, paddings also can be configured"* — sent with a
screenshot of `chrome = "dark"` on a terminal that is already black, where every panel came
out the colour the terminal already was and every panel's content started in column 0.

Both halves are the same finding said twice: **a frame reads as an application because its
regions are told apart, and a single frame-wide word cannot tell them apart** — whatever it
says, it says about all four panes at once.

The two halves are drawn by different things and the tests are split the same way:

* `bg` is tmux's. It is a pane option (`window-style`/`window-active-style`), so it costs
  nothing on a repaint, cannot wrap a pane, and survives a respawn. The whole risk is at
  the config boundary — a tmux style value is FORMAT-EXPANDED, so the operator's word must
  be a KEY into charter's table and never a value out of a committed file.
* `pad` is charter's. tmux paints backgrounds and insets nothing, so the inset is composed
  where the rows are, and the whole risk is arithmetic: the pad has to come OUT of the
  content budget (#597's `_row_plan`, #506's lost CI marker) rather than be added beside it
  (`chrome.fill`'s measured W+1, which shears the pane).
"""

from __future__ import annotations

import contextlib
import io
import os
import re
import subprocess
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

from charter import commands_frame, config, instance, statusline, tui
from charter.frame import gather, panel, slots

from tests import _tmuxchain
from tests._isolation import PersonaIso
from tests.test_frame_tmux_integration import _HAS_TMUX, _TmuxServerFixture


@contextlib.contextmanager
def _pane(cols: int, rows: int = 24):
    """A pane of exactly *cols* columns, measured the way a real panel measures its own.

    `slots._width` asks `os.get_terminal_size(sys.stdout.fileno())` — the pane, not
    `$COLUMNS` (#591) — so a test that wants a width has to fake the tty, which is the
    fixture `tests/test_frame_panel.py` and `tests/test_component_id_is_the_currency.py`
    already use for the same reason.
    """
    with mock.patch("os.get_terminal_size",
                    return_value=os.terminal_size((cols, rows))), \
         mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
        yield


def _arrangement(**style) -> dict:
    """The shipped four components, with *style* applied to the named ones.

    Keyed by component id (`repos`, `sidebar`, …), which is the vocabulary
    `[[frame.component]]` speaks. Resolved through `instance.frame_of` rather than built by
    hand: what the tests below then patch into `config.FRAME` is what a real charter.toml
    would have produced, refusals included.
    """
    tables = [{"use": cid, **style.get(cid, {})}
              for cid in ("identity", "attention", "repos", "sidebar")]
    return instance.frame_of({"frame": {"component": tables}})


class TheVocabularyIsClosedAndCharterOwnsEveryValue(unittest.TestCase):
    """`bg` is a WORD, and the containment `[frame] chrome` has is not relaxed by there
    being more words.

    Measured on tmux 3.7c: a style value is stored verbatim and evaluated at draw time, so
    `bg=#{?#{==:1,1},colour196,colour46}` reaches the wire as `ESC[48;5;196m`.
    `charter.toml` is committed and arrives from someone else's machine. So the operator's
    string is a dict key and the value handed to tmux comes out of charter's own table.
    """

    def test_the_vocabulary_is_default_and_the_sixteen_ansi_names(self):
        expected = {"default", *instance.FRAME_PANE_COLOURS,
                    *(f"bright{c}" for c in instance.FRAME_PANE_COLOURS)}
        self.assertEqual(set(instance.FRAME_PANE_BG), expected)
        self.assertEqual(len(expected), 17)

    def test_every_word_names_both_pane_options(self):
        """Both, always. A `bg` that set only `window-style` would leave the pane showing
        the frame-wide `chrome`'s colour when focused — one pane, two unrelated colours,
        which is a cell's worth of the two-colour defect #514 fixed on the borders."""
        for word, pairs in instance.FRAME_PANE_BG.items():
            with self.subTest(word=word):
                self.assertEqual(set(dict(pairs)),
                                 {"window-style", "window-active-style"})

    def test_a_colour_focuses_to_a_shade_off_itself(self):
        """§5.2's property: the live pane is a shade OFF the others. Not "lighter" — the
        direction reverses on the bright half because there is nothing brighter in the
        sixteen, and that is the property being asked rather than the spelling."""
        for word in instance.FRAME_PANE_BG:
            if word == "default":
                continue
            with self.subTest(word=word):
                pairs = dict(instance.FRAME_PANE_BG[word])
                self.assertNotEqual(pairs["window-style"],
                                    pairs["window-active-style"])

    def test_default_is_the_one_word_with_no_focus_shade(self):
        """Stated rather than worked around: `bg=default` is the terminal's own
        background, and it has no partner in the sixteen. `chrome = "off"`'s own answer,
        said about one pane instead of the whole frame."""
        pairs = dict(instance.FRAME_PANE_BG["default"])
        self.assertEqual(pairs["window-style"], "bg=default")
        self.assertEqual(pairs["window-active-style"], "bg=default")

    def test_a_style_string_is_refused_by_name(self):
        hostile = "bg=#{?#{==:1,1},colour196,colour46}"
        self.assertIsNone(instance.pane_bg(hostile))
        self.assertEqual(instance.pane_bg_options(hostile), ())

    def test_a_word_charter_does_not_know_is_refused(self):
        for value in ("chartreuse", "BLACK", "black ", "", "colour236", "bg=black"):
            with self.subTest(value=value):
                self.assertIsNone(instance.pane_bg(value))
                self.assertEqual(instance.pane_bg_options(value), ())

    def test_a_value_that_is_not_a_string_does_not_raise(self):
        """`tomllib` can hand this a list or a table, and `instance` is imported by every
        command including `charter --version`. `isinstance` first, for `chrome_level`'s
        own reason: `value in FRAME_PANE_BG` raises `TypeError` on an unhashable value."""
        for value in (["black"], {"bg": "black"}, 7, True, None, 1.5):
            with self.subTest(value=value):
                self.assertIsNone(instance.pane_bg(value))
                self.assertEqual(instance.pane_bg_options(value), ())

    def test_the_operators_word_is_only_ever_a_key(self):
        """**The containment stated as the property, not as the refusal.** `pane_bg` hands
        back the object it was given — for a `str` subclass out of a committed file, that
        subclass — and that is harmless precisely because the name is only ever a KEY: the
        pairs come out of `FRAME_PANE_BG`, so what a tmux evaluator sees is charter's own
        constant whatever the key's type was.

        A `str` whose `__str__` lies is the sharpest form of it. An implementation that
        built its value with `f"bg={name}"` would pass every refusal test above — the word
        `black` is admitted, correctly — and fail this one."""
        class Sneaky(str):
            def __str__(self) -> str:            # pragma: no cover - never reached
                return "bg=#{?1,colour196,colour46}"
        ours = {v for pairs in instance.FRAME_PANE_BG.values() for _n, v in pairs}
        for option, value in instance.pane_bg_options(Sneaky("black")):
            with self.subTest(option=option):
                self.assertIn(value, ours)
                self.assertIs(type(value), str)
                self.assertNotIn("#", value)

    def test_every_word_is_actually_read(self):
        """The control. A `pane_bg` that answered `None` for everything would pass every
        refusal above."""
        for word in instance.FRAME_PANE_BG:
            with self.subTest(word=word):
                self.assertEqual(instance.pane_bg(word), word)
                self.assertEqual(len(instance.pane_bg_options(word)), 2)


class ThePadIsBoundedAtTheConfigBoundary(unittest.TestCase):
    """`pad` is an integer with a cap, and the cap is a REFUSAL rather than a clamp.

    It reaches a repaint as `" " * n`, so `pad = 10**9` in a committed file is a panel
    process that stops answering. `FRAME_PANE_PAD_MAX` is the line that stops it being one.
    """

    def test_zero_through_the_maximum_are_admitted(self):
        for n in range(0, instance.FRAME_PANE_PAD_MAX + 1):
            with self.subTest(pad=n):
                self.assertEqual(instance.pane_pad(n), n)

    def test_zero_is_admitted_and_is_not_a_refusal(self):
        """`0` is falsy, so a caller spelling the check `if not pane_pad(v)` would refuse
        exactly the one value that means "no pad" — the spelling-not-property mistake this
        project has paid for six times. Asked as `is None`, asserted here."""
        self.assertIsNotNone(instance.pane_pad(0))
        self.assertEqual(instance.pane_pad(0), 0)

    def test_a_pad_above_the_cap_is_refused_and_not_reduced(self):
        for n in (instance.FRAME_PANE_PAD_MAX + 1, 100, 10 ** 9):
            with self.subTest(pad=n):
                self.assertIsNone(instance.pane_pad(n))

    def test_a_negative_pad_is_refused(self):
        """A negative pad would ADD to the content width and prefix nothing — a value
        that silently means the opposite of what it says."""
        for n in (-1, -8):
            with self.subTest(pad=n):
                self.assertIsNone(instance.pane_pad(n))

    def test_a_bool_is_refused_explicitly(self):
        """`isinstance(True, int)` is `True` in Python, so `pad = true` would otherwise
        be accepted and mean one cell."""
        for value in (True, False):
            with self.subTest(value=value):
                self.assertIsNone(instance.pane_pad(value))

    def test_a_value_that_is_not_an_int_is_refused(self):
        for value in ("2", 2.0, None, ["2"], {"pad": 2}):
            with self.subTest(value=value):
                self.assertIsNone(instance.pane_pad(value))


class TheCapIsTheNarrowestPanesOwnCeiling(unittest.TestCase):
    """`FRAME_PANE_PAD_MAX`'s VALUE, not just its existence.

    The deletion sweep asked the right question — *"is this pinned, or would any number
    do?"* — and the honest answer was that any number would. It is a derived number now:
    the largest pad the frame's own narrowest pane can afford. The sidebar is 22 columns,
    `slots._PAD_MIN_CONTENT` is 12, and a pad takes cells from BOTH sides.

    A cap above that admits a value `pad_for` always drops on the sidebar — which is one
    of the two panes the operator named, so it would buy exactly "your pad did nothing and
    nothing said why". The arithmetic lives here rather than in `instance` because
    `instance` is imported by every command and must not reach `frame/layout.py` at module
    scope; that is the trade `slots.INSET` already makes with `statusline._HEAD_PAD`.
    """

    def test_the_cap_is_what_the_sidebar_can_afford(self):
        from charter.frame import layout
        sidebar = layout.SLOT_SIZE["right"]
        self.assertEqual(instance.FRAME_PANE_PAD_MAX,
                         (sidebar - slots._PAD_MIN_CONTENT) // 2)

    def test_the_largest_admissible_pad_is_one_the_sidebar_actually_takes(self):
        """The property behind the arithmetic, asked of the function that decides. Not a
        second copy of the sum: this runs `pad_for` at the sidebar's real width."""
        from charter.frame import layout
        sidebar = layout.SLOT_SIZE["right"]
        with mock.patch.dict(
                config.FRAME,
                _arrangement(sidebar={"pad": instance.FRAME_PANE_PAD_MAX})):
            self.assertEqual(slots.pad_for("sidebar", sidebar),
                             instance.FRAME_PANE_PAD_MAX)

    def test_one_more_than_the_cap_would_be_dropped_by_that_pane(self):
        """The other side of the boundary, and the reason the cap is where it is: a pad
        one larger is one the sidebar could never honour."""
        from charter.frame import layout
        sidebar = layout.SLOT_SIZE["right"]
        over = instance.FRAME_PANE_PAD_MAX + 1
        self.assertLess(sidebar - 2 * over, slots._PAD_MIN_CONTENT)


class AnUnusableStyleTakesTheArrangementWithIt(unittest.TestCase):
    """#535's rule, extended to the two new keys rather than excepted from.

    Dropping the one bad key would hand the operator a pane that quietly lost the colour
    it asked for — a config value that changed nothing while claiming to decide something.
    The whole arrangement is refused, the frame falls back to `slots`, and the operator
    sees their arrangement ignored, which is visible.
    """

    def _fell_back(self, tables: list[dict]) -> None:
        f = instance.frame_of({"frame": {"component": tables}})
        self.assertEqual(f["components"], [])
        self.assertEqual(f["slots"], list(instance.FRAME_DEFAULTS["slots"]))

    def test_a_background_charter_does_not_know(self):
        for bad in ("chartreuse", "bg=#{?1,colour196,colour46}", "colour236", 7,
                    ["black"], True):
            with self.subTest(bg=bad):
                self._fell_back([{"use": "identity"}, {"use": "repos", "bg": bad}])

    def test_a_pad_charter_will_not_draw(self):
        for bad in (-1, instance.FRAME_PANE_PAD_MAX + 1, "2", 2.5, True, [2]):
            with self.subTest(pad=bad):
                self._fell_back([{"use": "identity"}, {"use": "repos", "pad": bad}])

    def test_a_key_that_is_not_in_the_form_at_all(self):
        """The closed key set still closes. `padding` is the near miss that matters —
        the plausible misspelling of the key this change adds."""
        for key in ("padding", "background", "colour", "bg_colour"):
            with self.subTest(key=key):
                self._fell_back([{"use": "identity"}, {"use": "repos", key: 2}])

    def test_the_boundary_check_is_the_only_answer_and_the_placement_is_not_a_second(self):
        """**Why there is no second sanitiser on the way into the placement.**

        `_placement(..., bg=pane_bg(bg), pad=pane_pad(pad))` was written first and deleted:
        both functions answer their own argument unchanged (asserted in
        `TheVocabularyIsClosedAndCharterOwnsEveryValue` and
        `ThePadIsBoundedAtTheConfigBoundary`), and the check three lines above has already
        turned away everything they would have turned away — so the call could not change a
        value and could not change an outcome. A line that cannot is a second, weaker
        answer to a question already answered, which is what #568 deleted the last of.

        This is what has to stay pinned for that deletion to be safe: what reaches a
        placement is either `None`/`0` or a word already in the table, **whatever the file
        said** — and it is asked of every word rather than of one, because "the hostile one
        is refused" is a spelling and "nothing else can get here" is the property."""
        for bad in ("chartreuse", "colour236", "bg=#{?1,colour196,colour46}"):
            with self.subTest(bg=bad):
                f = instance.frame_of({"frame": {"component": [
                    {"use": "repos", "bg": bad}]}})
                self.assertEqual(f["components"], [])
        for word in instance.FRAME_PANE_BG:
            for pad in range(0, instance.FRAME_PANE_PAD_MAX + 1):
                with self.subTest(bg=word, pad=pad):
                    f = _arrangement(repos={"bg": word, "pad": pad})
                    p = next(p for p in f["components"] if p["use"] == "repos")
                    self.assertIn(p["bg"], instance.FRAME_PANE_BG)
                    self.assertEqual(p["pad"], pad)
                    self.assertEqual(instance.pane_bg(p["bg"]), p["bg"])

    def test_a_usable_style_survives_and_reaches_the_placement(self):
        """The control, and it is not optional: every assertion above is a negative, and
        a `component_tables` that refused everything would pass all of them."""
        f = _arrangement(repos={"bg": "black", "pad": 2},
                         sidebar={"bg": "brightblack", "pad": 1})
        self.assertEqual(f["slots"], ["top", "bottom", "repos", "right"])
        by_use = {p["use"]: p for p in f["components"]}
        self.assertEqual((by_use["repos"]["bg"], by_use["repos"]["pad"]), ("black", 2))
        self.assertEqual((by_use["sidebar"]["bg"], by_use["sidebar"]["pad"]),
                         ("brightblack", 1))
        self.assertEqual((by_use["identity"]["bg"], by_use["identity"]["pad"]), (None, 0))


class OneWalkOverTheArrangement(unittest.TestCase):
    """`instance.component_style` — asked by the launcher for the colour and by a panel
    process for the pad, and it has to be one function.

    Two membership walks over `frame["components"]` is the shape #547 cost: they come to
    disagree about which name matches, and here the two callers are different processes
    that never compare notes.
    """

    def test_either_spelling_finds_the_same_component(self):
        """A component travels under two names — the committed slot name (`right`) and
        the component id (`sidebar`) — and both are live: the launcher splits on slot
        names and a panel process is started with whichever name the arrangement used."""
        f = _arrangement(sidebar={"bg": "blue", "pad": 3})
        for name in ("sidebar", "right"):
            with self.subTest(name=name):
                self.assertEqual(instance.component_style(f, name),
                                 {"bg": "blue", "pad": 3})

    def test_a_name_nothing_declares_gets_the_empty_style(self):
        f = _arrangement(sidebar={"bg": "blue"})
        for name in ("repos", "top", "acme.metrics", "", None):
            with self.subTest(name=name):
                self.assertEqual(instance.component_style(f, name),
                                 {"bg": None, "pad": 0})

    def test_the_shorthand_forms_place_components_with_no_style_at_all(self):
        """**`_placement`'s own defaults, reached rather than restated.**

        `_built_in_placement` used to spell out `bg=None, pad=0` beside `_placement`'s
        identical defaults, and the hand-check found what that costs: mutating
        `_placement`'s `pad` default to `1` changed nothing anywhere, because every call
        arriving through that function passed a `0` of its own. Two defaults for one thing,
        the second hiding the first.

        `frame_components` is the path that reaches them — `slots`, `density` and the
        shipped default all place built-ins with no style argument at all — so this is
        where "a plane that never wrote per-pane style has none" is asked.
        """
        for cfg in ({}, {"frame": {"slots": ["top", "right"]}},
                    {"frame": {"density": "minimal"}}):
            with self.subTest(cfg=cfg):
                placed = instance.frame_components(cfg)
                self.assertTrue(placed)
                for p in placed:
                    self.assertIsNone(p["bg"])
                    self.assertEqual(p["pad"], 0)

    def test_a_plane_spelled_with_slots_has_no_per_pane_style(self):
        """Per-pane style is written in `[[frame.component]]`; a plane that has not
        written one gets exactly the frame it had."""
        f = instance.frame_of({"frame": {"slots": ["top", "repos"]}})
        self.assertEqual(f["components"], [])
        for name in ("top", "repos", "right"):
            with self.subTest(name=name):
                self.assertEqual(instance.component_style(f, name),
                                 {"bg": None, "pad": 0})

    def test_the_arrangement_is_always_placement_dicts(self):
        """**What makes `component_style`'s deleted `isinstance(placed, dict)` safe.**

        That guard was written and the sweep found it surviving, correctly: every entry in
        `frame["components"]` is a dict by construction, because `component_tables` is the
        only thing that fills the list and `_placement`'s return is the only thing it
        appends. That is a contract, so it is asserted here rather than defended there —
        asked of every shape a committed `[[frame.component]]` can take, including the ones
        that make `component_tables` answer `None`.
        """
        cases = [
            [{"use": "identity"}, {"use": "repos", "bg": "blue", "pad": 1}],
            [{"use": "identity"}, {"use": "repos", "visible": False}],
            [{"use": "identity"}, "not a table"],
            [{"use": "identity"}, {"use": "identity"}],
            [{"use": "nope.nothing", "edge": "right", "size": 9}],
            [{"use": "repos", "bg": "chartreuse"}],
            [{"use": "repos", "pad": -1}],
            ["identity"], [], [7],
        ]
        for tables in cases:
            with self.subTest(tables=tables):
                f = instance.frame_of({"frame": {"component": tables}})
                self.assertIsInstance(f["components"], list)
                for placed in f["components"]:
                    self.assertIsInstance(placed, dict)
                    self.assertEqual({"use", "slot", "edge", "size", "visible", "key",
                                      "bg", "pad"}, set(placed))

    def test_a_placement_with_no_pad_and_one_with_a_null_pad_read_the_same(self):
        """Two shapes for "no pad" — a placement from a charter that predates the key, and
        one carrying the key with nothing in it — and both have to answer `0` rather than
        `None`, because what reads this multiplies it (`" " * n`)."""
        for placed in ({"slot": "right", "use": "sidebar"},
                       {"slot": "right", "use": "sidebar", "pad": None}):
            with self.subTest(placed=placed):
                self.assertEqual(
                    instance.component_style({"components": [placed]}, "right"),
                    {"bg": None, "pad": 0})

    def test_a_frame_with_no_components_key_at_all_does_not_raise(self):
        """A frame relaunched by a charter that predates this key has a resolved config
        without it — `_split_panels` already reads `chrome` with `.get` for that reason."""
        self.assertEqual(instance.component_style({}, "repos"), {"bg": None, "pad": 0})
        self.assertEqual(instance.component_style({"components": None}, "repos"),
                         {"bg": None, "pad": 0})


class ThePaneWearsItsOwnColourAndTheFrameWearsTheRest(unittest.TestCase):
    """`_surface_argvs`: a component's `bg` wins whole, and everything else is unchanged.
    """

    def test_a_components_background_replaces_both_options(self):
        argvs = commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="dark",
                                              bg="blue")
        self.assertEqual([a[-2:] for a in argvs],
                         [["window-style", "bg=blue"],
                          ["window-active-style", "bg=brightblue"]])

    def test_a_component_with_no_background_gets_the_frames_own(self):
        """The control that the existing behaviour survived: `bg=None` is every pane on
        every plane written before this key existed."""
        argvs = commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="dark",
                                              bg=None)
        # Against the ASSEMBLER and not against `FRAME_CHROME`'s bare values: #737 pairs a
        # foreground with charter's own two surfaces, and `instance.surface_options` is the
        # one place the two halves meet. A test spelling the background table here would be
        # asserting half of what the pane wears.
        self.assertEqual([a[-1] for a in argvs],
                         [v for _n, v in instance.surface_options(
                             None, "dark", instance.SHIPPED_LOOK)])

    def test_the_default_parameter_is_no_background(self):
        """Called without `bg` at all — which every existing call site and every test
        written before this change does — and the answer is the frame's own."""
        self.assertEqual(
            commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="dark"),
            commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="dark",
                                          bg=None))

    def test_a_background_does_not_need_chrome_to_be_on(self):
        """The smallest way to answer the report that started this: leave `chrome` at its
        shipped `off` and colour one pane. `off` is a default, not a prohibition — and a
        `bg` is not a default, it is a line somebody wrote by hand about one pane."""
        argvs = commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="off",
                                              bg="brightblack")
        self.assertEqual([a[-1] for a in argvs], ["bg=brightblack", "bg=black"])

    def test_a_word_charter_does_not_know_falls_back_to_the_frame(self):
        """Two answers, and this is the second of them: the arrangement carrying such a
        word was already refused whole at the config boundary, so nothing can reach here
        with one. If something did, it produces no style of its own."""
        argvs = commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="dark",
                                              bg="bg=#{?1,colour196,colour46}")
        self.assertEqual([a[-1] for a in argvs],
                         [v for _n, v in instance.surface_options(
                             None, "dark", instance.SHIPPED_LOOK)])
        for a in argvs:
            self.assertNotIn("#", a[-1])

    def test_no_operator_string_reaches_tmux(self):
        """The property, asked of the whole vocabulary: every value in every argv is one
        of charter's own constants. Not "the hostile one is refused" — that is a spelling
        — but "nothing charter did not write can appear here"."""
        ours = {v for pairs in instance.FRAME_PANE_BG.values() for _n, v in pairs}
        ours |= {v for pairs in instance.FRAME_CHROME.values() for _n, v in pairs}
        # The paired foregrounds (#737) are charter's own constants too, and the property
        # is "nothing charter did not write can appear here" — so the vocabulary widens
        # with the table rather than the assertion being loosened. Derived from both
        # tables, so a surface word added to either is inside this line on the same commit.
        ours |= {f"{fg},{v}" for fg in instance.FRAME_CHROME_FG.values() if fg
                 for pairs in instance.FRAME_CHROME.values() for _n, v in pairs}
        for word in (*instance.FRAME_PANE_BG, "chartreuse", "colour236",
                     "bg=#{?1,colour196,colour46}", None, 7, ["black"]):
            with self.subTest(bg=word):
                for a in commands_frame._surface_argvs(socket="s", pane_id="%3",
                                                       chrome="dark", bg=word):
                    self.assertIn(a[-1], ours)

    def test_no_color_refuses_a_components_background_too(self):
        """The half that is easy to miss: the fill is tmux's paint, not charter's, so
        gating only charter's own SGR would leave an operator who asked for no colour
        looking at a coloured frame — charter having asked somebody else to paint it."""
        for value in ("", "0", "1"):
            with self.subTest(value=value), \
                 mock.patch.dict(os.environ, {"NO_COLOR": value}, clear=True):
                self.assertEqual(
                    commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="off",
                                                  bg="blue"), [])

    def test_without_no_color_a_background_is_issued(self):
        """The control for the negative above."""
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(
                len(commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="off",
                                                  bg="blue")), 2)


class TheLauncherActuallyAsksForEachPanesColour(PersonaIso, unittest.TestCase):
    """**The wiring, which every `_surface_argvs` test above takes as given.**

    The hand-check found this gap by replacing `_split_panels`' one resolving line with
    `bg = None` and watching the whole suite stay green: `_surface_argvs` was exercised
    with a `bg` handed to it, and nothing asked whether production ever hands it one. A
    per-pane colour that is never resolved is a feature that does nothing on a real frame
    and a test file that says it works.

    So this drives the real funnel — `_split_panels`, which both launch paths and every
    density change come through — and reads back the argv it issued for each pane.
    """

    def _issued(self, frame: dict, slots: list[str]) -> list[list[str]]:
        """Every tmux argv `_split_panels` issues for *slots*, with tmux itself faked.

        Only `tmuxctl.run` is replaced. `layout.panel_argvs` and `_surface_argvs` are the
        production functions, so what is recorded is what charter would really have run.
        """
        seen: list[list[str]] = []
        panes = iter(f"%{n}" for n in range(10, 99))

        def fake_run(_why, argv, **_kw):
            # ONE entry per tmux COMMAND and one id per `split-window`, on its own line:
            # since #780 charter sends the splits as one command list and a pane's own
            # options as another, and real tmux answers a chain of splits with one id per
            # line, in order (measured on 3.7c and at the 3.2 floor).
            issued = _tmuxchain.commands(argv)
            seen.extend(issued)
            out = "".join(f"{next(panes)}\n" for c in issued if "split-window" in c)
            return subprocess.CompletedProcess(argv, 0, stdout=out, stderr="")

        with mock.patch.dict(config.FRAME, frame), \
             mock.patch.object(commands_frame.tmuxctl, "run", fake_run), \
             mock.patch.dict(os.environ, {}, clear=True):
            commands_frame._split_panels("sock", slots=slots, fid="f-1",
                                         harness_pane="%1", env=None, pane_env=None)
        return seen

    def _styles(self, issued: list[list[str]]) -> dict[str, list[str]]:
        """`{pane id: [style values]}` out of the recorded `set-option -p` calls.

        **Filtered to the options this class is about, by the family tmux itself names
        them with** — an option whose name ends `style` — rather than by every `-p` the
        funnel happens to issue. `test_frame_launcher._is_chrome` makes the same call for
        the same reason: `_split_panels` is a shared funnel, and a helper here that
        collected whatever it issued would turn every future pane option into a failure in
        four tests that are not about it. #634's `@charter_panel` mark was exactly that —
        a pane option about mouse ROUTING, landing in a list of colours as a bare `'1'`.
        """
        out: dict[str, list[str]] = {}
        for argv in issued:
            if "set-option" in argv and "-p" in argv and argv[-2].endswith("style"):
                out.setdefault(argv[argv.index("-t") + 1], []).append(argv[-1])
        return out

    def test_each_pane_is_styled_with_its_own_components_colour(self):
        frame = _arrangement(repos={"bg": "blue"}, sidebar={"bg": "brightblack"})
        issued = self._issued(frame, ["repos", "right"])
        styles = list(self._styles(issued).values())
        self.assertEqual(styles, [["bg=blue", "bg=brightblue"],
                                  ["bg=brightblack", "bg=black"]])

    def test_a_pane_whose_component_named_no_colour_gets_the_frames_chrome(self):
        """The other half of the `or`, driven through the funnel rather than the helper."""
        frame = dict(_arrangement(repos={"bg": "blue"}), chrome="dark")
        styles = self._styles(self._issued(frame, ["repos", "right"]))
        dark = [v for _n, v in instance.surface_options(None, "dark",
                                                        instance.SHIPPED_LOOK)]
        self.assertEqual(list(styles.values()), [["bg=blue", "bg=brightblue"], dark])

    def test_the_harness_pane_is_never_a_target(self):
        """ADR 0018, asked of the funnel: `-p` never names the harness. Its window options
        (`set-option -w`) are a different scope and are supposed to name it."""
        frame = _arrangement(repos={"bg": "blue"}, sidebar={"bg": "red"})
        for argv in self._issued(frame, ["repos", "right"]):
            if "set-option" in argv and "-p" in argv:
                with self.subTest(argv=argv):
                    self.assertNotIn("%1", argv)

    def test_a_plane_with_no_per_pane_colour_issues_exactly_what_it_did_before(self):
        """The control that this whole mechanism is inert until somebody writes a `bg`."""
        bare = instance.frame_of({"frame": {"chrome": "dark"}})
        styles = self._styles(self._issued(bare, ["repos", "right"]))
        # Both panes answered for, asserted before the loop rather than left to it: this
        # case's body is a loop over what `_styles` found, so an empty answer would pass it
        # without running an assertion at all. That became reachable when `_styles` started
        # filtering to the style family (#634) — before, every pane had at least one `-p`
        # to report. The two cases above would still fail, but a control that cannot fail
        # is not one.
        self.assertEqual(len(styles), 2,
                         f"both panes must have been styled, not {sorted(styles)}")
        dark = [v for _n, v in instance.surface_options(None, "dark",
                                                        instance.SHIPPED_LOOK)]
        for values in styles.values():
            self.assertEqual(values, dark)


class ThePadComesOutOfTheBudget(PersonaIso, unittest.TestCase):
    """The arithmetic half, and the one thing about it that could not be decided by taste.

    `statusline._row_plan` gives up whole cells in a written-down order when the repo
    table is narrow, and #506 is what a row composed for one width and painted at another
    costs: the CI marker falls off the right-hand end and a failing repo renders clean. A
    pad added on top of a full-width plan is that defect with a new cause. At the other
    end `chrome.fill`'s measurement says W+1 shears the pane — one cell of overflow wraps
    every row below it onto the next line.

    So the renderer is told a narrower pane. The assertions below are the two directions
    of that: what a padded pane composes, and what it must never exceed.
    """

    def setUp(self):
        super().setUp()
        self.make_persona("alice")
        gather.save("f-1", {
            "gathered_at": 0.0, "workspace": "w", "current_repo": "demo",
            "repos": [{"name": "demo", "branch": "main", "dirty": True,
                       "tracked_dirty": True, "ahead": 2, "behind": 0, "ci": "failed",
                       "change": None, "sigil": "●", "current": True,
                       "worktree_count": 0}],
            "worktrees": [], "todos": [{"title": "ship it"}], "todo_count": 1})

    def _render(self, slot: str, *, cols: int, frame: dict) -> str:
        with mock.patch.dict(config.FRAME, frame), _pane(cols):
            return slots.render(slot, "f-1")

    def test_a_padded_pane_composes_exactly_what_a_narrower_pane_composes(self):
        """**The whole claim, as one equality.** A `repos` pane of 120 columns with
        `pad = 2` draws the same table as a `repos` pane of 116 columns with no pad, moved
        two cells right. Not "looks similar" — byte-identical once the lead is removed,
        because the renderer was handed the same number and made the same plan.

        This is what "the pad comes out of the budget" MEANS, and it is the assertion a
        pad added beside the budget would fail: there the 120-column pane would compose a
        120-column table and then be pushed off its own right edge.
        """
        padded = self._render("repos", cols=120,
                              frame=_arrangement(repos={"pad": 2}))
        narrow = self._render("repos", cols=116, frame=_arrangement())
        self.assertEqual([ln.removeprefix("  ") for ln in padded.split("\n")],
                         narrow.split("\n"))
        for ln in padded.split("\n"):
            self.assertTrue(ln.startswith("  "), repr(ln))

    def test_the_pad_actually_changed_the_pane(self):
        """The control for the equality above, which would also hold if `pad` did
        nothing at all and both sides rendered at 120."""
        padded = self._render("repos", cols=120, frame=_arrangement(repos={"pad": 2}))
        plain = self._render("repos", cols=120, frame=_arrangement())
        self.assertNotEqual(padded, plain)

    def test_no_row_of_a_padded_pane_exceeds_the_pane(self):
        """W+1 shears the pane (`chrome.fill`, measured in a real 20-column tmux pane:
        one cell of overflow wraps every row below it). Asked across every slot, every
        pad and a spread of widths, because a pad is exactly the way an off-by-one gets
        added to a width that used to be right."""
        for cols in (40, 60, 96, 120, 200):
            for pad in range(0, instance.FRAME_PANE_PAD_MAX + 1):
                frame = _arrangement(**{cid: {"pad": pad}
                                        for cid in ("identity", "attention", "repos",
                                                    "sidebar")})
                for slot in sorted(slots.SLOTS):
                    with self.subTest(cols=cols, pad=pad, slot=slot):
                        out = self._render(slot, cols=cols, frame=frame)
                        for ln in out.split("\n"):
                            self.assertLessEqual(tui.width(tui.strip_ansi(ln)), cols)

    def test_content_width_is_the_pane_less_both_pads(self):
        with mock.patch.dict(config.FRAME, _arrangement(repos={"pad": 3})), _pane(120):
            self.assertEqual(slots._width(), 120)
            self.assertEqual(slots.content_width("repos"), 114)
            self.assertEqual(slots.content_width("top"), 120)

    def test_width_itself_still_answers_the_pane(self):
        """`_width` is the rectangle and `content_width` is the canvas, and the two must
        not merge: `panel._hold` paints a failure into a pane whose renderer is the thing
        that failed, and it needs the rectangle."""
        with mock.patch.dict(config.FRAME, _arrangement(repos={"pad": 8})), _pane(60):
            self.assertEqual(slots._width(), 60)

    def test_inset_rows_moves_every_row_and_only_leftwards(self):
        with mock.patch.dict(config.FRAME, _arrangement(repos={"pad": 2})), _pane(120):
            got = slots.inset_rows("a\nbb\n", "repos")
        self.assertEqual(got, "  a\n  bb\n  ")

    def test_every_row_shape_these_panes_take_is_inset_the_same_way(self):
        """`inset_rows` is spelled with `replace` rather than a split and a join, because
        the split form carries an equivalent mutant nothing can pin (`split`/`rsplit` with
        no `maxsplit` are one function). The two spellings have to agree, so the cases that
        could tell them apart are asked here rather than argued in a docstring: a blank
        line, a trailing newline, a bare `\\r` — which `splitlines` WOULD have split on and
        `split("\\n")` does not — and no newline at all.

        Written as the property (every line starts with the lead, and stripping the lead
        gives the original lines back) rather than against the other implementation, so it
        stays meaningful once that implementation is gone."""
        with mock.patch.dict(config.FRAME, _arrangement(repos={"pad": 2})), _pane(120):
            for text in ("", "a", "a\nb", "a\n", "\n\n", "\n", "a\n\nb\n", "x\ry"):
                with self.subTest(text=text):
                    got = slots.inset_rows(text, "repos")
                    lines = got.split("\n")
                    self.assertEqual(len(lines), len(text.split("\n")))
                    for ln in lines:
                        self.assertTrue(ln.startswith("  "), repr(ln))
                    self.assertEqual([ln[2:] for ln in lines], text.split("\n"))

    def test_inset_rows_is_the_identity_at_no_pad(self):
        """`" " * 0` is `""` and the join is the split's inverse, so a pane with no pad
        gets back the value it was given — trailing newline included.

        **This is the property, and there is no early return implementing it.** `if not
        pad: return text` was written, the deletion sweep found it surviving, and it was
        deleted rather than pinned: it could not change an outcome. So these cases are
        what keeps that true through the general path — the whole point of the deletion is
        that they stay green without the line."""
        with mock.patch.dict(config.FRAME, _arrangement()), _pane(120):
            for text in ("", "a", "a\nb", "a\n", "\n\n", "\n", "a\n\nb\n"):
                with self.subTest(text=text):
                    self.assertEqual(slots.inset_rows(text, "repos"), text)

    def test_an_unpadded_pane_renders_byte_for_byte_what_it_did_before(self):
        """The deletion's real exit criterion, said at the level an operator sees: with no
        `pad` anywhere, every slot draws exactly what a plane with no `[[frame.component]]`
        style at all draws. If the split-and-join round trip were not the identity, this is
        where it would show — a trailing blank row appearing or disappearing."""
        for slot in sorted(slots.SLOTS):
            with self.subTest(slot=slot), _pane(120):
                with mock.patch.dict(config.FRAME, _arrangement()):
                    styled = slots.render(slot, "f-1")
                with mock.patch.dict(config.FRAME,
                                     instance.frame_of({"frame": {"slots": list(
                                         instance.FRAME_DEFAULTS["slots"])}})):
                    bare = slots.render(slot, "f-1")
                self.assertEqual(styled, bare)

    def test_the_pad_lands_outside_every_style_span(self):
        """The pad is background, not paint. A row that ends with a span still OPEN — a
        `chrome.reverse` highlight — keeps its highlight to the content's last column and
        no further, so a selected row is inset like every other row rather than bleeding
        into the margin. Asked of the composed row, because that is where it is visible."""
        from charter.frame import chrome
        with mock.patch.dict(config.FRAME, _arrangement(sidebar={"pad": 2})), _pane(40):
            got = slots.inset_rows(chrome.reverse("hi", 10), "right")
        self.assertTrue(got.startswith("  \x1b["), repr(got))
        self.assertNotIn("\x1b", got[:2])


class APadThePaneCannotAffordIsDroppedWhole(PersonaIso, unittest.TestCase):
    """Not clamped down to fit, and the difference is what an operator can tell.

    A clamp is a value read, validated and then quietly changed into a different one —
    `FRAME_PANE_PAD_MAX` refuses rather than reduces at the config boundary, and this is
    the same rule one layer down where the pane's own width is what runs out. Whole-or-
    nothing is also what the table beside it already keeps for a marker
    (`statusline._row_plan`: shown whole or dropped whole).
    """

    def _pad_of(self, name: str, *, cols: int, pad: int) -> int:
        with mock.patch.dict(config.FRAME,
                             _arrangement(**{name: {"pad": pad}})), _pane(cols):
            return slots.pad_of(name)

    def test_a_pad_the_pane_can_afford_is_taken_whole(self):
        self.assertEqual(self._pad_of("repos", cols=120, pad=3), 3)

    def test_a_pad_that_would_leave_less_than_the_floor_is_dropped_to_zero(self):
        """Not to 1, not to 2. `_PAD_MIN_CONTENT` cells of content or none of the pad."""
        floor = slots._PAD_MIN_CONTENT
        self.assertEqual(self._pad_of("repos", cols=floor + 5, pad=3), 0)

    def test_the_floor_is_inclusive(self):
        """Exactly `_PAD_MIN_CONTENT` cells left is afforded; one fewer is not. Two
        assertions either side of one boundary, because `>=` and `>` differ by exactly
        this case and a test that only asked one side could not tell them apart."""
        floor = slots._PAD_MIN_CONTENT
        self.assertEqual(self._pad_of("repos", cols=floor + 6, pad=3), 3)
        self.assertEqual(self._pad_of("repos", cols=floor + 5, pad=3), 0)

    def test_the_floor_is_the_narrowest_row_the_frame_composes(self):
        """Read from `_NAME_MIN_W` rather than spelled, so the number that stops a pad
        and the number the sidebar squeezes a persona name to are the same one."""
        self.assertEqual(slots._PAD_MIN_CONTENT, slots._NAME_MIN_W)

    def test_a_dropped_pad_leaves_the_pane_exactly_as_it_was(self):
        """The point of dropping it whole: the narrow pane draws what it drew before
        anybody wrote a `pad`, rather than a slightly-inset version of it."""
        cols = slots._PAD_MIN_CONTENT + 5          # 4 a side would leave 9 — under it
        with _pane(cols):
            with mock.patch.dict(config.FRAME, _arrangement(sidebar={"pad": 4})):
                padded = slots.render("right", "f-1")
            with mock.patch.dict(config.FRAME, _arrangement()):
                plain = slots.render("right", "f-1")
        self.assertEqual(padded, plain)

    def test_a_pane_too_narrow_to_hold_anything_does_not_raise(self):
        """A pane narrower than the pad itself: the arithmetic goes negative before the
        comparison, and the answer has to be "no pad" rather than a traceback."""
        for cols in (1, 2, 3, 4):
            with self.subTest(cols=cols):
                self.assertEqual(self._pad_of("repos", cols=cols, pad=2), 0)

    def test_a_pane_charter_cannot_measure_gets_the_stated_default_and_its_pad(self):
        """**A zero is not a size (#606), and the pad does not second-guess that.**

        `pane.size()` answers `None` for a stream that cannot be asked *and* for a tty that
        answers with a zero, and `slots._width` turns that into `_DEFAULT_COLS` — one
        stated fallback rather than a measurement nobody made. So a pad on such a pane is
        afforded against 80, exactly as every other width decision on that pane is. Asked
        here because the obvious reading is the other one: `pad_of` looks like it is doing
        arithmetic on a real width, and on this pane it is not."""
        with mock.patch.dict(config.FRAME, _arrangement(repos={"pad": 2})), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((0, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            self.assertEqual(slots._width(), slots._DEFAULT_COLS)
            self.assertEqual(slots.pad_of("repos"), 2)
            self.assertEqual(slots.content_width("repos"), slots._DEFAULT_COLS - 4)


class TheSizerAndTheRendererAgreeAboutThePad(PersonaIso, unittest.TestCase):
    """#500's defect said with a pad in it: the launcher must size the `repos` pane from
    the width the RENDERER will see.

    `_repos` composes at `content_width`, so a padded pane's table is planned for
    `pane_cols - 2 * pad`. A sizer asking `_table_cap` for the unpadded width answers "a
    table fits, give it eight rows" in exactly the band where the renderer then draws one
    line saying it is too narrow — a tall pane with a complaint in it. That is the shape
    #500 shipped twice, once from the window's width and once from the pane's.
    """

    def setUp(self):
        super().setUp()
        gather.save("f-1", {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                            "repos": [{"name": "demo", "branch": "main", "dirty": False,
                                       "tracked_dirty": False, "ahead": 0, "behind": 0,
                                       "ci": None, "change": None, "sigil": "●",
                                       "current": True, "worktree_count": 0}],
                            "worktrees": [], "todos": [], "todo_count": 0})

    def test_the_sizer_asks_for_no_rows_where_the_renderer_would_refuse(self):
        """The band: a pane exactly at `_LEFT_W` with `pad = 2` leaves 91 cells, which is
        under the table's floor. The sizer must answer 0 there, not eight rows."""
        with mock.patch.dict(config.FRAME, _arrangement(repos={"pad": 2})):
            self.assertEqual(
                slots.repos_rows_wanted("f-1", pane_cols=statusline._LEFT_W), 0)

    def test_without_a_pad_the_same_pane_is_sized_as_it_always_was(self):
        """The control: the band is the pad's, not a change to what an unpadded frame
        does at that width."""
        with mock.patch.dict(config.FRAME, _arrangement()):
            self.assertGreater(
                slots.repos_rows_wanted("f-1", pane_cols=statusline._LEFT_W), 0)

    def test_the_sizer_and_the_renderer_answer_the_same_question(self):
        """Asked across the whole band rather than at one point: for every width either
        side of the table's floor, "the sizer wanted rows" and "the renderer drew a table"
        must be the same boolean. A second copy of the pad arithmetic on either side shows
        up here as a width where they disagree."""
        frame = _arrangement(repos={"pad": 3})
        lo, hi = statusline._LEFT_W - 2, statusline._LEFT_W + 8
        for cols in range(lo, hi):
            with self.subTest(cols=cols), mock.patch.dict(config.FRAME, frame):
                sized = slots.repos_rows_wanted("f-1", pane_cols=cols) > 0
                with _pane(cols):
                    drew = "too narrow" not in tui.strip_ansi(
                        slots.render("repos", "f-1"))
                self.assertEqual(sized, drew,
                                 f"sizer says {sized}, renderer says {drew}")

    def test_pad_for_is_the_function_pad_of_is(self):
        """One function, two widths — not two functions that must be kept in step. The
        renderer measures its pane; the launcher is handed a width for a pane that does
        not exist yet and could only measure the operator's own shell."""
        with mock.patch.dict(config.FRAME, _arrangement(repos={"pad": 3})):
            for cols in (14, 17, 18, 40, 200):
                with self.subTest(cols=cols), _pane(cols):
                    self.assertEqual(slots.pad_of("repos"),
                                     slots.pad_for("repos", cols))


class TheTooNarrowLineQuotesWhatThePaneNeeds(PersonaIso, unittest.TestCase):
    """`repos` refuses to draw a cut table below `statusline._LEFT_W` and says so. With a
    pad in play the number it says has to be the width the PANE needs, not the table's.

    Otherwise: a 97-column pane with `pad = 3` refuses, quotes 95, the operator widens a
    terminal that is already wide enough, it still refuses, and the config key that caused
    it is not on screen anywhere.
    """

    def test_the_unpadded_line_is_exactly_what_it_was(self):
        line = tui.strip_ansi(slots._too_narrow_lines(60)[0])
        self.assertIn(f"{statusline._LEFT_W} columns needed", line)

    def test_the_padded_line_adds_both_pads(self):
        line = tui.strip_ansi(slots._too_narrow_lines(60, 3)[0])
        self.assertIn(f"{statusline._LEFT_W + 6} columns needed", line)

    def test_the_number_is_one_the_operator_can_act_on(self):
        """End to end: a pane one cell short of what the padded table needs says so, and
        the number it says is a width at which the table actually draws."""
        self.make_persona("alice")
        gather.save("f-1", {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                            "repos": [{"name": "demo", "branch": "main", "dirty": False,
                                       "tracked_dirty": False, "ahead": 0, "behind": 0,
                                       "ci": None, "change": None, "sigil": "●",
                                       "current": True, "worktree_count": 0}],
                            "worktrees": [], "todos": [], "todo_count": 0})
        needed = statusline._LEFT_W + 6
        frame = _arrangement(repos={"pad": 3})
        with mock.patch.dict(config.FRAME, frame):
            with _pane(needed - 1):
                short = slots.render("repos", "f-1")
            with _pane(needed):
                wide = slots.render("repos", "f-1")
        self.assertIn(f"{needed} columns needed", tui.strip_ansi(short))
        self.assertNotIn("too narrow", tui.strip_ansi(wide))


class APaddedPaneStartsInOneColumn(PersonaIso, unittest.TestCase):
    """§5.4 said again with the pad in front of it: every row's content starts in the same
    column, and that column is now the pad plus `slots.INSET`.

    The operator's screenshot is the reason this is asserted rather than assumed — every
    panel's content flush against column 0 is what they saw.
    """

    def setUp(self):
        super().setUp()
        self.make_persona("alice")
        self.make_persona("bob")
        gather.save("f-1", {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                            "repos": [], "worktrees": [],
                            "todos": [{"title": "ship it"}], "todo_count": 1})

    def test_every_sidebar_row_starts_past_the_pad(self):
        with mock.patch.dict(config.FRAME, _arrangement(sidebar={"pad": 2})), _pane(40):
            out = slots.render("right", "f-1")
        for ln in out.split("\n"):
            plain = tui.strip_ansi(ln)
            if not plain.strip():
                continue
            with self.subTest(row=plain):
                self.assertTrue(plain.startswith("  "), repr(plain))

    def test_the_pad_is_added_to_the_inset_rather_than_replacing_it(self):
        """Two different things. `INSET` is the column content starts in WITHIN the pane —
        `▸ steward` sits two cells in from the pane's own left edge, and every renderer
        reads that one constant. `pad` is how far the pane's content sits from the pane's
        EDGE. A pad that swallowed the inset would flatten the sidebar's own alignment
        while looking, from the outside, like it had worked.

        So a persona row in a pane with `pad = 3` starts at column 5: three cells of pad,
        then the two `INSET` gives it."""
        with mock.patch.dict(config.FRAME, _arrangement(sidebar={"pad": 3})), _pane(40):
            padded = slots.render("right", "f-1").split("\n")
        rows = [tui.strip_ansi(ln) for ln in padded if tui.strip_ansi(ln).strip()]
        self.assertTrue(rows, "the sidebar drew nothing to check")
        for row in rows:
            with self.subTest(row=row):
                self.assertTrue(row.startswith(" " * 3), repr(row))
                self.assertNotEqual(row[3 + slots.INSET], " ",
                                    "content did not start at pad + INSET")


class ThePadSurvivesNoColour(PersonaIso, unittest.TestCase):
    """`NO_COLOR` turns off colour. It does not turn off layout.

    `panel._write` strips every SGR under it (`chrome.plain`), and a frame that lost its
    inset there would be answering a question about colour with a change to layout — which
    is the argument `chrome.recipes` makes about `slots.INSET` one key over.
    """

    def _painted(self, slot: str, *, cols: int, frame: dict, env: dict) -> str:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err), \
             mock.patch.dict(os.environ, env, clear=True), \
             mock.patch.dict(config.FRAME, frame), _pane(cols, 6):
            panel.run(slot, "f-1", once=True)
        return out.getvalue().split("\x1b[2J", 1)[-1]

    def test_a_padded_pane_keeps_its_pad_with_no_color_set(self):
        self.make_persona("alice")
        gather.save("f-1", {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                            "repos": [], "worktrees": [], "todos": [], "todo_count": 0})
        painted = self._painted("top", cols=80,
                                frame=_arrangement(identity={"pad": 2}),
                                env={"NO_COLOR": "1"})
        self.assertNotIn("\x1b[", painted)
        self.assertTrue(painted.startswith("  "), repr(painted[:20]))


class AProvidersComponentIsPaddedToo(PersonaIso, unittest.TestCase):
    """A stranger's component gets the operator's pad without opting in to anything.

    That is the point of the pad being charter's to draw: `ctx.width` is what a provider
    is told, so the pad comes out THERE, and a provider that was told the whole pane and
    then moved right by the pad would be composing for a width it does not have —
    `chrome.fill`'s measured W+1, which shears the pane.
    """

    def setUp(self):
        super().setUp()
        from tests.test_component_id_is_the_currency import _installed
        _installed(self, render="lambda ctx: ['w=%d' % ctx.width]")

    def test_the_provider_is_told_the_padded_width_and_its_rows_are_inset(self):
        from tests.test_component_providers import CID
        from charter.frame import builtins
        frame = instance.frame_of({"frame": {"component": [
            {"use": "identity"},
            {"use": CID, "edge": "right", "size": 30, "pad": 2}]}})
        self.assertNotEqual(frame["components"], [], "the arrangement was refused")
        reg = builtins.build()
        reg.place(CID)
        with mock.patch.dict(config.FRAME, frame), _pane(40, 6):
            got = panel._component_text(reg, CID, "f-1")
        self.assertEqual(got, "  w=36")


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class RealTmuxAcceptsEveryWordAndPaintsPerPane(_TmuxServerFixture, PersonaIso):
    """Read back out of a real tmux rather than intended in charter.

    A refused `set-option` is silent in production — reported, not fatal — so a word tmux
    does not parse would ship as a pane that simply never coloured.
    """

    SOCKET_NAME = f"charter-pane-style-{os.getpid()}"

    def _panes(self) -> tuple[str, str, str]:
        r = self._srv("new-session", "-d", "-s", "h", "-x", "120", "-y", "24",
                      "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness = r.stdout.strip()
        out = [harness]
        for _ in range(2):
            p = self._srv("split-window", "-t", harness, "-P", "-F", "#{pane_id}",
                          "--", "sleep", "600")
            self.assertEqual(p.returncode, 0, p.stderr)
            out.append(p.stdout.strip())
        return tuple(out)

    def _style(self, pane: str, option: str = "window-style") -> str:
        return self._srv("show", "-p", "-t", pane, "-v", option).stdout.strip()

    def _apply(self, pane: str, *, chrome: str = "off", bg=None) -> None:
        for argv in commands_frame._surface_argvs(socket=self.SOCKET_NAME,
                                                  pane_id=pane, chrome=chrome, bg=bg):
            r = subprocess.run(argv, capture_output=True, text=True, timeout=10)
            self.assertEqual(r.returncode, 0, f"{argv}: {r.stderr}")

    def test_tmux_parses_every_word_in_the_vocabulary(self):
        _harness, pane, _other = self._panes()
        for word in instance.FRAME_PANE_BG:
            with self.subTest(word=word):
                self._apply(pane, bg=word)
                self.assertEqual(self._style(pane),
                                 dict(instance.FRAME_PANE_BG[word])["window-style"])

    def test_two_panes_come_out_two_different_colours(self):
        """The operator's actual ask, read back from tmux: the frame's panes are
        distinguishable from each other, which is what one frame-wide word cannot do."""
        harness, a, b = self._panes()
        self._apply(a, chrome="dark", bg="blue")
        self._apply(b, chrome="dark", bg="brightblack")
        self.assertEqual(self._style(a), "bg=blue")
        self.assertEqual(self._style(b), "bg=brightblack")
        self.assertNotEqual(self._style(a), self._style(b))
        self.assertEqual(self._style(harness), "",
                         "charter styled the pane the operator's harness runs in")

    def test_a_pane_can_opt_out_of_a_frame_wide_chrome(self):
        """`bg = "default"` is the terminal's own background — how one pane steps out of
        `chrome = "dark"` without the other three doing so."""
        _harness, a, b = self._panes()
        self._apply(a, chrome="dark")
        self._apply(b, chrome="dark", bg="default")
        # `fg=white,bg=black` — the frame-wide surface with the foreground #737 pairs with
        # it. `b` opted out and therefore gets neither half, which is the line
        # `FRAME_CHROME_FG` draws: the pairing belongs to the background it came with.
        self.assertEqual(self._style(a), dict(instance.surface_options(
            None, "dark", instance.SHIPPED_LOOK))["window-style"])
        self.assertEqual(self._style(b), "bg=default")

    def test_the_colour_belongs_to_the_pane_and_not_to_the_process_in_it(self):
        """`commands_frame`'s `pane-died` hook respawns a dead panel INTO THE SAME PANE.
        These are pane options, so a per-component colour is a property of the rectangle —
        the same claim `chrome` already makes, asked of the new key because a renderer-side
        fill would have to be re-established by whatever came back."""
        _harness, pane, _other = self._panes()
        self._srv("set", "-g", "remain-on-exit", "on")
        self._apply(pane, bg="blue")
        r = self._srv("respawn-pane", "-k", "-t", pane, "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self._style(pane), "bg=blue")


class TheDocumentedExampleIsRunThroughCharter(unittest.TestCase):
    """`docs/frame.md` is where an operator reads what they may write, and the example
    they will paste is **parsed and resolved here** rather than eyeballed.

    A documented `charter.toml` that charter refuses is worse than no example: the whole
    arrangement falls back to `slots` and the operator sees a frame that ignored the file
    they copied from the docs. Read out of the markdown, through `tomllib`, through the
    real `instance.frame_of` — so a key renamed in the code and not in the docs is red.
    """

    def _blocks(self) -> list[str]:
        md = (Path(__file__).resolve().parents[1] / "docs" / "frame.md").read_text()
        start = md.index("### A colour and an inset for one pane")
        end = md.index("### Writing the arrangement out", start)
        return re.findall(r"```toml\n(.*?)```", md[start:end], re.S)

    def test_the_section_exists_and_carries_an_example(self):
        """The control for the loop below, which passes vacuously on zero blocks."""
        self.assertEqual(len(self._blocks()), 1)

    def test_the_documented_example_resolves_to_the_frame_it_describes(self):
        import tomllib
        cfg = tomllib.loads(self._blocks()[0])
        f = instance.frame_of(cfg)
        self.assertEqual(f["slots"], ["top", "bottom", "repos", "right"],
                         "the documented arrangement was refused whole")
        got = {p["use"]: (p["bg"], p["pad"]) for p in f["components"]}
        self.assertEqual(got, {"identity": (None, 0), "attention": (None, 0),
                               "repos": ("black", 1), "sidebar": ("brightblack", 1)})

    def test_the_seventeen_words_are_named_in_the_documentation(self):
        """An operator cannot guess a closed vocabulary. Asked of the table rather than of
        a sentence, so a colour added to `FRAME_PANE_BG` and not to the docs is red."""
        md = (Path(__file__).resolve().parents[1] / "docs" / "frame.md").read_text()
        for word in instance.FRAME_PANE_BG:
            if word.startswith("bright"):
                continue        # named once as the rule, not sixteen times as a list
            with self.subTest(word=word):
                self.assertIn(f"`{word}`", md)
        self.assertIn("`bright` forms", md)


class EveryDocumentedArrangementIsAWholeArrangement(unittest.TestCase):
    """#690. **`[[frame.component]]` REPLACES the arrangement, so a snippet that lists one
    table is an instruction to turn everything else off.**

    The reported one was the chat bar: *"Turn one on with a `[[frame.component]]` table"*
    over a fence naming `chats` and nothing else. Pasted into a plane's `charter.toml`,
    that resolves to a frame whose entire contents are the chat bar — no identity row, no
    attention strip, no repo table, no sidebar — because a component list is the whole
    arrangement (`instance.component_tables`) and not an addition to the shipped one. The
    caveat existed, seven hundred lines further down in the toggle-keys section, which is
    not where the reader is being told to write the table.

    The property asked here is the operator's, not the prose's: **paste any fence in this
    file and you still have charter's four panels.** A sentence of warning above the fence
    would fix the report and could not be checked, because a reader who stops at the code
    block never reads it; a fence that lists the whole arrangement cannot be misread and
    is exactly what this asserts.

    Related and asserted together: #687 — a `size` charter refuses on ANY table in the
    list takes the whole arrangement out of play in silence, so a documented number is
    checked here by resolving it, not by reading it.
    """

    MD = Path(__file__).resolve().parents[1] / "docs" / "frame.md"

    #: Charter's own four, in the vocabulary `use` speaks. Written out rather than read
    #: from `builtins.SLOT_OF` on purpose: this is what the DOCUMENTATION must show an
    #: operator, and a fifth built-in becoming placeable by default is a documentation
    #: change, not something this test should absorb silently.
    SHIPPED = ("identity", "attention", "repos", "sidebar")

    def _fences(self) -> list[tuple[int, str]]:
        """Every ```toml fence in `docs/frame.md` that writes a component table, with the
        line it starts on so a failure says where to look."""
        md = self.MD.read_text()
        return [(md[:m.start()].count("\n") + 1, m.group(1))
                for m in re.finditer(r"```toml\n(.*?)```", md, re.S)
                if "[[frame.component]]" in m.group(1)]

    def test_the_documentation_carries_arrangements_to_check(self):
        """The vacuity control: both loops below pass on a file with no examples in it,
        and a snippet quietly deleted is exactly how this check would stop meaning
        anything."""
        self.assertGreaterEqual(len(self._fences()), 7,
                                "docs/frame.md lost `[[frame.component]]` examples")

    def test_every_documented_arrangement_lists_charters_own_four(self):
        """#690 itself. Asked of every fence, because the reported one was not special —
        it was the first of four partial lists in the file."""
        for line, body in self._fences():
            import tomllib
            uses = [t.get("use")
                    for t in tomllib.loads(body)["frame"]["component"]]
            with self.subTest(line=line):
                missing = [c for c in self.SHIPPED if c not in uses]
                self.assertEqual(
                    missing, [], f"docs/frame.md:{line} shows a `[[frame.component]]` "
                    f"arrangement that omits {missing}. An operator who pastes it turns "
                    f"those panels OFF — the list replaces the frame rather than adding "
                    f"to it — and the frame they get is {uses}.")

    def test_every_arrangement_of_charters_own_resolves_to_what_it_shows(self):
        """Stronger than reading the fence: resolved through the real `instance.frame_of`,
        so a documented `size`, `bg`, `pad`, `edge` or `key` charter would refuse is red
        here rather than in an operator's terminal.

        A refusal is what makes this worth running. `component_tables` refuses an
        arrangement WHOLE (#535) and says nothing about it (`instance.py`'s `_HOTKEY_RE`
        note), so a documented number charter cannot honour does not cost the reader the
        line they got wrong — it costs them every panel in the file.

        Fences naming a component no installed distribution supplies are skipped: the
        provider example is about a package that does not exist on this machine, and
        `frame_of` is right to refuse it. Its SHAPE is still checked by the test above.
        """
        import tomllib

        from charter.frame import builtins as _builtins
        ours = {c.id for c in _builtins.build().all()}
        checked = 0
        for line, body in self._fences():
            cfg = tomllib.loads(body)
            uses = [t.get("use") for t in cfg["frame"]["component"]]
            if not set(uses) <= ours:
                continue
            checked += 1
            with self.subTest(line=line):
                got = [p["use"] for p in instance.frame_of(cfg)["components"]]
                self.assertEqual(
                    got, uses, f"docs/frame.md:{line} does not resolve to the arrangement "
                    f"it prints. An empty list here means charter REFUSED the whole thing "
                    f"and fell back to `slots`, which is what an operator pasting it would "
                    f"silently get.")
        self.assertGreaterEqual(checked, 6, "nothing was resolved — the skip ate the loop")


#: A number written as a number of CELLS, and not a component of a version.
#:
#: The distinction is the one that matters, and it is a property rather than a markup
#: spelling: ``3.2`` and ``3.7c`` are tmux versions this file names constantly, and a
#: digit with a decimal point and another digit on the far side of it is one of those.
#: Everything else — `` `5` ``, ``5``, ``5.`` at the end of a sentence, ``5,`` in a list —
#: is a count, and is held to `instance.pane_pad`.
#:
#: The two negative lookaheads are not one lookahead, and the lookbehinds mirror them.
#: ``(?![.\d])`` — which is what #675 shipped inside its backtick-anchored reader —
#: refuses a number followed by ANY dot, so it silently skips every number that ends a
#: sentence: "The maximum `pad` is 12." was invisible to it. ``(?!\.\d)`` refuses only a
#: dot with a digit after it, which is what "this is a version" actually means; ``(?!\d)``
#: is separately what stops `12` being read out of `120`.
_CELL_COUNT = r"(?<!\d)(?<!\d\.)(\d+)(?!\d)(?!\.\d)"

#: A bound written as a RANGE, in every spelling this file could plausibly use, with the
#: backticks OPTIONAL. #689: the backticks were the whole match before, so the same
#: sentence #669 filed — "a `pad` outside `0`-`8` is refused" — went green the moment its
#: digits were written bare. Markup is not the property; the pair of numbers is.
_STATED_RANGE = re.compile(
    r"(?:(?P<between>between)\s+)?`?" + _CELL_COUNT + r"`?\s*"
    r"(?:(?(between)and|(?!))|to|through|[-–—])\s*`?" + _CELL_COUNT + r"`?")

#: `pad`, `pads`, `padded`, `padding` — the word, in the forms prose actually uses, and
#: NOT the literal `` `pad` ``. The leading lookbehind is what keeps ``trackpad`` (which
#: this file says once, about mouse wheels) out of the reader's scope.
_PAD_WORD = re.compile(r"(?<![A-Za-z])pad(?:s|ded|ding)?(?![A-Za-z])", re.I)

_NUMBER = re.compile(_CELL_COUNT)
_SENTENCE = re.compile(r"(?<=[.!?])\s+")
_FENCE = re.compile(r"```.*?```", re.S)
_HEADING = re.compile(r"(?m)^#{1,6} .*$")


def prose_of(md: str) -> str:
    """*md* with its fenced blocks and its headings taken out.

    Fences go because `TheDocumentedExampleIsRunThroughCharter` and
    `EveryDocumentedArrangementIsAWholeArrangement` put every one of them through
    `instance.frame_of` itself, which is a stronger question than any reader of English.
    Headings go so that flattening the file cannot glue a heading to the sentence under it.
    """
    return _HEADING.sub("", _FENCE.sub("", md))


def stated_ranges(prose: str) -> list[tuple[int, int]]:
    """**Every** range of cells *prose* states — not the first one, and not whether one
    exists.

    That distinction is the whole of #669. `docs/frame.md` gave `pad` two ranges four
    hundred lines apart — `` `0` to `5` `` and `` `0`–`8` `` — and the test asked
    ``assertIn(f"`0` to `{FRAME_PANE_PAD_MAX}`", md)``, which the *right* copy satisfied.
    An assertion that one occurrence exists cannot see a second, wrong one; the wrong one
    was the sentence about refusal, so `pad = 6` on the documentation's word silently cost
    the operator their whole `[[frame.component]]` block.

    Unscoped on purpose: a range of bare cell counts is a thing this file states about
    `pad` and about nothing else, so the reader does not have to work out what a sentence
    is about before it can hold the pair to the constant. A second such bound arriving is
    a reason to teach this function the second constant.
    """
    return [(int(m.group(2)), int(m.group(3)))
            for m in _STATED_RANGE.finditer(" ".join(prose.split()))]


def pad_numbers(prose: str) -> list[tuple[int, str]]:
    """Every cell count *prose* offers as a `pad`, with the text it was read out of.

    **Scope is the property "this text is about `pad`", and it is two readings unioned**,
    because #689 is what one narrow reading costs. #675 scoped this to *a sentence
    containing the literal* `` `pad` ``, so "A pane may carry a `pad`. Charter caps it at
    `12`." was green: the number and the word were one sentence apart.

    * **the paragraph** that names a `pad` word — the document's own unit of topic, so a
      bound stated five sentences below the word is still in reach;
    * **one sentence either side** of each `pad` word, over the file FLATTENED — so a
      bound stated in the paragraph after the one that names the key is in reach too.

    **What this cannot catch, said plainly rather than implied away.** No reader of English
    prose is complete, and these are the shapes that get past this one:

    * a bound written in WORDS — "charter caps a `pad` at twelve" has no integer in it;
    * a bound more than one sentence from any form of the word `pad`, in a paragraph that
      never says it — "the inset ceiling is 12", alone, under a heading about insets;
    * anything inside a fenced block, which `prose_of` removes — those are resolved
      through `instance.frame_of` instead, which is stricter than this;
    * a wrong bound stated somewhere other than `docs/frame.md`.

    `TheReaderIsAttackedWithTheSpellingsThatBeatTheLastOne` runs the corpus that IS in
    reach, and `test_the_shapes_this_reader_is_blind_to_are_the_ones_written_down` runs
    the ones that are not, so both halves of that claim are measured rather than asserted.

    **The cost, accepted knowingly.** Scoping by proximity instead of by markup means an
    incidental integer near a `pad` sentence is a false red. `docs/frame.md` carries none
    today (measured), and the rule it puts on the file is writable: an incidental number
    beside the `pad` prose is spelled in words or moved. The judgement #689 named is that
    this noise is cheaper than a reader that misses the sentence #669 was filed about.
    """
    scoped: list[str] = []
    for paragraph in re.split(r"\n\s*\n", prose):
        flat = " ".join(paragraph.split())
        if flat and _PAD_WORD.search(flat):
            scoped.append(flat)
    sentences = _SENTENCE.split(" ".join(prose.split()))
    near = {i + offset
            for i, s in enumerate(sentences) if _PAD_WORD.search(s)
            for offset in (-1, 0, 1)}
    scoped += [sentences[i] for i in sorted(near) if 0 <= i < len(sentences)]
    # One entry per NUMBER, quoting the shortest text it was found in: the two readings
    # overlap by design — a sentence is inside its paragraph — and a caller wants the
    # number once, with the tightest quotation of where it came from for its message.
    where: dict[int, str] = {}
    for text in scoped:
        for n in (int(x) for x in _NUMBER.findall(text)):
            if n not in where or len(text) < len(where[n]):
                where[n] = text
    return sorted(where.items())


#: Restatements of the `pad` bound, one per plausible spelling, with the number left as a
#: hole. **This tuple is the attack, kept as data so the next person inherits it rather
#: than repeating it.**
#:
#: #689 was measured by appending restatements of a WRONG bound to `docs/frame.md` and
#: watching the suite stay green. Three of these are that reviewer's own three, verbatim;
#: the rest are the same defect in the spellings a documentation edit would plausibly
#: reach for. Each one is run twice — with `FRAME_PANE_PAD_MAX` in the hole, where it must
#: be GREEN, and with a number charter refuses, where it must be RED — because a reader
#: that fails everything catches nothing.
#:
#: Adding a spelling here is the cheap half of a fix. If one arrives that neither reader
#: catches, widen the reader; if it cannot be widened without noise, add it to
#: `_OUT_OF_REACH` instead, so the gap is written down rather than absent.
_BOUND_RESTATEMENTS = (
    # The three from #689's own reproduction table.
    "The `pad` key insets a pane's content. Values from 0 to {n} are accepted.",
    "A pane may carry a `pad`. Charter caps it at `{n}`.",
    "Any `pad` outside 0-{n} refuses the whole arrangement.",
    # #669's own refusal sentence, in the spelling that was caught and the two that were not.
    "A `pad` outside `0`-`{n}` is refused.",
    "A `pad` outside `0`–`{n}` is refused.",
    "a pad outside 0 to {n} is refused",
    # A cap rather than a range — the phrasing #675 claimed was already red.
    "pad is capped at {n}",
    "The maximum `pad` is {n}.",
    "The pad ceiling is {n}.",
    "Charter refuses a `pad` over {n}.",
    "Padding is limited to {n} cells.",
    "A padded pane may inset by as much as {n} cells.",
    # The same bound one paragraph away from the word, which sentence scope cannot see.
    "A pane may carry a `pad`.\n\nCharter caps it at `{n}`.",
    # And the same bound several sentences BELOW the word inside one paragraph, which the
    # sentence window cannot see either. This is the line the paragraph reading exists
    # for: delete that reading and every other line here is still caught.
    ("A pane may carry a `pad`. It is one number, and it means both sides. "
     "It comes out of the pane's own width rather than your terminal's. "
     "Charter caps it at `{n}`."),
    # Range spellings this file does not use today but an edit could.
    "`pad` accepts `0` through `{n}`.",
    "A `pad` between 0 and {n} is honoured.",
    "Pads run 0—{n}.",
    # Not a sentence at all.
    "| `pad` | `0` to `{n}` | how many cells |",
    "`pad = {n}` is accepted.",
    "Give the sidebar a `pad` of {n}.",
)

#: The shapes neither reader catches, kept as data for the same reason the corpus above
#: is: a limit that is measured cannot quietly stop being true, and a guard that overstates
#: its reach is the thing this project refuses.
#:
#: If a line here starts being caught — because someone widened `pad_numbers` — the test
#: that runs it goes RED. That is the intended signal, and the fix is to move the line up
#: into `_BOUND_RESTATEMENTS`, not to re-narrow the reader.
_OUT_OF_REACH = (
    # A bound spelled in words has no integer for any of this to hold to the constant.
    "Charter caps a `pad` at twelve.",
    # A bound in a paragraph that never names the key, more than one sentence from one.
    "The inset ceiling is {n}. It is not a round number. It was picked to fit.",
)


class EveryStatementOfThePadBoundIsHeldToTheConstant(unittest.TestCase):
    """#669. `FRAME_PANE_PAD_MAX` is written out in prose more than once, and every copy
    has to agree with the constant — all of them, not one of them.

    Scoped to `docs/frame.md`, the way `test_readme_states_the_ceilings` is scoped to the
    README: it is the file an operator reads to learn what they may write, and the file the
    enforcement sentence lives in. A news entry records what a release said and is not held
    to a constant that may move after it.
    """

    MD = Path(__file__).resolve().parents[1] / "docs" / "frame.md"

    def setUp(self) -> None:
        self.md = self.MD.read_text()

    def test_the_reader_finds_the_second_copy_the_old_assertion_could_not(self):
        """The control, and the reason `stated_ranges` is a function.

        Run over the exact pair #669 reported — the right copy and the wrong one. `assertIn`
        on the first was green with the second sitting in the same file, so a reader that
        returned only the first would rebuild the defect."""
        both = ("`0` to `5`. Five is not a round number picked by hand …\n"
                "… and so are a `bg` that is not one of the seventeen words, a `pad` "
                "outside `0`–`8`, and a `size` charter cannot give the component")
        self.assertEqual(stated_ranges(both), [(0, 5), (0, 8)])

    def test_a_version_is_not_a_range_of_cells_and_a_full_stop_is_not_a_decimal_point(self):
        """The two halves of `_CELL_COUNT`, which is where #689's second defect lived.

        Version ranges must stay out: this file says "on tmux 3.2 to 3.6" and dropping the
        backtick requirement without the dotted-number rule reads that as `2` to `3`. That
        is the reason #689 gives for why the obvious widening is not a one-character fix,
        so it is asserted rather than trusted.

        And a number that ENDS a sentence must stay in. #675's reader excluded any digit
        followed by a dot, which is every count at the end of an English sentence — so
        "The maximum `pad` is 12." was invisible to it, and that is a spelling a
        documentation edit reaches for constantly."""
        for versions in ("On tmux 3.2 to 3.6", "On tmux `3.2` to `3.6`",
                         "Measured on 3.1c, 3.2 and 3.7c", "at the 3.2 floor"):
            with self.subTest(versions=versions):
                self.assertEqual(stated_ranges(versions), [])
                self.assertEqual(pad_numbers(f"A `pad`. {versions}"), [])
        self.assertEqual([n for n, _ in pad_numbers("The maximum `pad` is 12.")], [12])
        self.assertEqual(stated_ranges("Pads run 0—12."), [(0, 12)])
        # 395 ms is a bare count and IS in reach — the reader excludes versions, not
        # every number that happens to sit near one. This is the noise `pad_numbers`
        # documents as the price of scoping by proximity, shown rather than described.
        self.assertEqual([n for n, _ in pad_numbers("A `pad`. median 395 ms on 3.7c")],
                         [395])

    def test_the_word_and_not_the_markup_puts_a_sentence_in_scope(self):
        """`pad` unbackticked, `padding`, `padded`, `pads` — all the same key. And
        `trackpad`, which this file says once about mouse wheels, is not: the reader is
        scoped by the WORD, so a lookbehind is what keeps that sentence's numbers out."""
        for spelling in ("a pad of 12", "padding of 12", "a padded 12", "pads of 12",
                         "a `pad` of 12"):
            with self.subTest(spelling=spelling):
                self.assertEqual([n for n, _ in pad_numbers(spelling)], [12])
        self.assertEqual(pad_numbers("the horizontal wheel a trackpad reports, 12 of them"),
                         [])

    def test_the_documentation_states_the_bound_at_all(self):
        """The vacuity control: the assertions below are green on a file that says
        nothing, so the file is asked to still be saying it."""
        prose = prose_of(self.md)
        self.assertTrue(stated_ranges(prose),
                        "docs/frame.md states no range — the `pad` bound went missing")
        self.assertTrue(pad_numbers(prose),
                        "docs/frame.md prints no number about `pad` any more")

    def test_every_range_the_documentation_states_is_the_pad_range(self):
        """The fix. Each copy is checked, so the enforcement sentence at the bottom of the
        file is in reach whatever the copy up by the `pad` prose says.

        `pad` is the only bound this file states as a range of cells. A second one arriving
        is a reason to teach this test the second constant — never a reason to go back to
        asking whether one of them appears somewhere."""
        want = (0, instance.FRAME_PANE_PAD_MAX)
        wrong = [r for r in stated_ranges(prose_of(self.md)) if r != want]
        self.assertEqual(
            wrong, [],
            f"docs/frame.md states {wrong} where charter enforces {want}. #669 was two "
            "copies of this range disagreeing, and the harmful copy was the sentence about "
            "refusal: an arrangement charter cannot draw is refused WHOLE, so a `pad` the "
            "documentation invited costs the operator the entire `[[frame.component]]` "
            "block and the frame falls back to `slots`.")

    def test_no_number_the_documentation_offers_as_a_pad_is_one_charter_refuses(self):
        """The copy that is not written as a range — "a `pad` of 8", "capped at 8" — held
        to `pane_pad` itself rather than to a spelling of the constant.

        The rule this puts on the file: **do not print a bare integer in prose about
        `pad` that charter would refuse.** An incidental number that lands there is
        spelled in words or moved, which is the noise `pad_numbers` documents as the price
        of scoping by proximity rather than by markup."""
        refused = [(n, text) for n, text in pad_numbers(prose_of(self.md))
                   if instance.pane_pad(n) is None]
        self.assertEqual(
            refused, [],
            f"docs/frame.md offers {sorted({n for n, _ in refused})} as a `pad` and charter "
            f"refuses it (the cap is {instance.FRAME_PANE_PAD_MAX}): "
            + " | ".join(f"…{t[:120]}" for _, t in refused[:3]))


class TheReaderIsAttackedWithTheSpellingsThatBeatTheLastOne(unittest.TestCase):
    """#689. **The fix for #669 was itself an instance of #669**: a guard that matched a
    markup spelling where the property was a bound, filed against a defect that was a guard
    matching a spelling where the property was a bound. Fifth measured instance in this
    repository of a fix for a class of bug containing that bug.

    So the reader above is not asserted to work — it is ATTACKED, the way the reviewer
    attacked #675's: a restatement of a wrong bound is appended to the real
    `docs/frame.md`, the readers are run over the result, and it has to come out red. The
    corpus is `_BOUND_RESTATEMENTS` and lives beside the readers as data, so the next
    person inherits the attack rather than repeating it.

    Both directions are run. A reader that fails everything catches nothing, so every
    spelling is run again with `FRAME_PANE_PAD_MAX` in the hole and must be GREEN — which
    is also what makes the corpus survive the constant moving.
    """

    MD = Path(__file__).resolve().parents[1] / "docs" / "frame.md"

    def _verdict(self, appended: str) -> list[str]:
        """What the two readers say about `docs/frame.md` with *appended* added to it."""
        prose = prose_of(f"{self.MD.read_text()}\n\n{appended}\n")
        want = (0, instance.FRAME_PANE_PAD_MAX)
        found = [f"range {r}" for r in stated_ranges(prose) if r != want]
        found += [f"number {n}" for n, _ in pad_numbers(prose)
                  if instance.pane_pad(n) is None]
        return found

    def test_the_shipped_documentation_is_green(self):
        """The control the whole class rests on: every red below has to be caused by the
        appended sentence and not by the file it was appended to."""
        self.assertEqual(self._verdict(""), [])

    def test_a_wrong_bound_is_caught_in_every_spelling_the_corpus_carries(self):
        """The attack. Three of these are #689's own reproduction table, which #675 left
        green; the rest are the same defect in the spellings an edit would reach for.

        Three wrong numbers each, and they are not decoration: `MAX + 1` is the off-by-one
        an edit makes, and a large one is the copy that came from a different constant."""
        for template in _BOUND_RESTATEMENTS:
            for wrong in (instance.FRAME_PANE_PAD_MAX + 1,
                          instance.FRAME_PANE_PAD_MAX + 7, 97):
                sentence = template.format(n=wrong)
                with self.subTest(sentence=sentence):
                    self.assertTrue(
                        self._verdict(sentence),
                        f"docs/frame.md can state a `pad` bound of {wrong} as "
                        f"{sentence!r} and both readers stay green. That is #689 again: "
                        "the reader is matching something other than the bound.")

    def test_the_same_spellings_with_the_right_bound_stay_green(self):
        """The other direction, and the reason the corpus is templated rather than
        literal. A reader that reds on every sentence containing a number would pass the
        test above while being useless, and it would make the file uneditable."""
        for template in _BOUND_RESTATEMENTS:
            sentence = template.format(n=instance.FRAME_PANE_PAD_MAX)
            with self.subTest(sentence=sentence):
                self.assertEqual(
                    self._verdict(sentence), [],
                    f"{sentence!r} states the bound charter enforces and the reader "
                    "called it wrong — a false red makes this check the kind that gets "
                    "deleted rather than fixed.")

    def test_an_enumeration_is_caught_number_by_number(self):
        """Not a bound at all: a list of the values that are allowed. Every member is held
        to `pane_pad`, so the one over the cap is the one that reds — which is the
        difference between asking the enforcing function and asking about a range."""
        cap = instance.FRAME_PANE_PAD_MAX
        listing = "A `pad` may be " + ", ".join(str(i) for i in range(cap + 3)) + "."
        self.assertTrue(self._verdict(listing))
        honest = "A `pad` may be " + ", ".join(str(i) for i in range(cap + 1)) + "."
        self.assertEqual(self._verdict(honest), [])

    def test_the_shapes_this_reader_is_blind_to_are_the_ones_written_down(self):
        """**The honesty test.** `pad_numbers` says in its docstring what it cannot catch,
        and this runs those shapes to prove the list is the real one rather than a
        disclaimer. A guard that overstates its reach is the thing this project refuses.

        Going red here means a shape moved from *out of reach* to *caught* — someone
        widened the reader. That is good news: move the line into `_BOUND_RESTATEMENTS`.
        It must never be answered by narrowing the reader back."""
        for template in _OUT_OF_REACH:
            sentence = template.format(n=instance.FRAME_PANE_PAD_MAX + 7)
            with self.subTest(sentence=sentence):
                self.assertEqual(
                    self._verdict(sentence), [],
                    f"{sentence!r} is listed as out of reach and was caught. Move it into "
                    "_BOUND_RESTATEMENTS and shorten the docstring's list of limits.")


class TheLiveChromeToggleDoesNotEraseAPanesOwnColour(PersonaIso, unittest.TestCase):
    """#604 landed `charter frame-chrome <level>` — the palette repaints a running frame's
    surface — while this branch was in flight. The two meet on the same two pane options,
    and the meeting had a defect in it.

    `_resurface_argvs` resolved only the frame-wide word. So a component with
    `bg = "brightblack"` was repainted in the frame's colour by `chrome: light`, and
    **erased** by `chrome: off` — whose whole job is to unset exactly the two options a
    per-pane colour is made of — with nothing to bring it back until relaunch. A value
    written in a committed file, silently undone by a keystroke.

    The property, and the reason this is not just "add a parameter": **the launch path and
    the live path must answer the same question the same way.** Two answers is #547's
    shape, here on a surface the operator is looking at.
    """

    #: `-u` sits BEFORE `-t`, so a helper that sliced from `-t` would hide exactly the
    #: flag that distinguishes a removal from a setting — and every "did it unset?"
    #: assertion below would then be asking about a list it had already thrown away.
    #: Whole argvs are kept and `_tail` is only for reading a failure message.
    def _live(self, *, chrome: str, bg=None) -> list[list[str]]:
        with mock.patch.dict(os.environ, {}, clear=True):
            return commands_frame._resurface_argvs(socket="s", pane_id="%3",
                                                   chrome=chrome, bg=bg)

    def _launch(self, *, chrome: str, bg=None) -> list[list[str]]:
        with mock.patch.dict(os.environ, {}, clear=True):
            return commands_frame._surface_argvs(socket="s", pane_id="%3",
                                                 chrome=chrome, bg=bg)

    @staticmethod
    def _tail(argvs: list[list[str]]) -> list[list[str]]:
        return [a[a.index("-t") + 1:] for a in argvs]

    def test_a_components_colour_survives_every_chrome_level(self):
        for level in sorted(instance.FRAME_CHROME):
            with self.subTest(chrome=level):
                self.assertEqual(self._tail(self._live(chrome=level, bg="brightblack")),
                                 [["%3", "window-style", "bg=brightblack"],
                                  ["%3", "window-active-style", "bg=black"]])

    def test_chrome_off_does_not_unset_a_colour_the_component_asked_for(self):
        """The sharpest case: `off`'s job is to REMOVE the two options, and those are
        exactly the two a per-pane colour is made of."""
        argvs = self._live(chrome="off", bg="blue")
        self.assertFalse([a for a in argvs if "-u" in a],
                         "the pane's own colour was unset by `chrome: off`")

    def test_a_pane_with_no_colour_of_its_own_still_gets_the_unsets(self):
        """The control — without it a `_resurface_argvs` that never unset anything would
        pass the test above, and `chrome: off` would be a keypress that changes nothing."""
        argvs = commands_frame._resurface_argvs(socket="s", pane_id="%3", chrome="off")
        self.assertEqual(len(argvs), len(instance.chrome_option_names()))
        for a in argvs:
            self.assertIn("-u", a)

    def test_the_live_path_and_the_launch_path_agree_about_every_pane(self):
        """The property both functions have to keep, asked across the whole cross-product
        rather than at one point. Where the launch path sets something, the live path must
        set the same thing; it may only add REMOVALS, which is the one thing it is for."""
        for level in sorted(instance.FRAME_CHROME):
            for bg in (None, "blue", "brightblack", "default"):
                with self.subTest(chrome=level, bg=bg):
                    launch = self._launch(chrome=level, bg=bg)
                    live = [a for a in self._live(chrome=level, bg=bg) if "-u" not in a]
                    self.assertEqual(live, launch)
                    # And the removals only ever name options nothing is setting.
                    for a in self._live(chrome=level, bg=bg):
                        if "-u" in a:
                            self.assertNotIn(a[-1], [x[-2] for x in launch])

    def test_no_color_still_beats_both(self):
        with mock.patch.dict(os.environ, {"NO_COLOR": "1"}, clear=True):
            argvs = commands_frame._resurface_argvs(socket="s", pane_id="%3",
                                                    chrome="dark", bg="blue")
        self.assertTrue(argvs, "NO_COLOR on a running frame means the unsets, not silence")
        for a in argvs:
            self.assertIn("-u", a)

    def test_the_command_itself_asks_each_pane_for_its_own_colour(self):
        """**The wiring, not the helper** — and this is the second time the hand-check has
        had to say so on this branch. `_resurface_argvs` is exercised above with a `bg`
        handed to it; nothing there asks whether `cmd_chrome` ever hands it one. Replacing
        that one lookup with `bg = None` left every assertion above green, exactly as the
        same shape did for `_split_panels`.

        So this drives `charter frame-chrome off` for real, against a recorded pane map,
        and reads back what each pane was told: the sidebar keeps the colour its component
        asked for, and the pane that asked for nothing gets the removals `off` means.
        """
        from charter.frame import state
        ran: list[list[str]] = []
        with mock.patch.object(commands_frame.tmuxctl, "run",
                               _tmuxchain.recorder(ran)):
            state.record_panes("fr-pane-style", panels={"repos": "%1", "right": "%2"})
            with mock.patch.dict(config.FRAME,
                                 _arrangement(sidebar={"bg": "brightblack"})), \
                 mock.patch.dict(os.environ,
                                 {"CHARTER_SESSION_ID": "fr-pane-style"}, clear=True):
                self.assertEqual(
                    commands_frame.cmd_chrome(type("A", (), {"level": "off"})()), 0)
        told: dict[str, list[list[str]]] = {}
        for a in ran:
            told.setdefault(a[a.index("-t") + 1], []).append(a)
        self.assertEqual(sorted(told), ["%1", "%2"])
        # `right` is the sidebar: its own colour, SET, and nothing unset.
        self.assertEqual([a[-2:] for a in told["%2"]],
                         [["window-style", "bg=brightblack"],
                          ["window-active-style", "bg=black"]])
        # `repos` named no colour: `off` means remove, which is what it must still do.
        self.assertTrue(all("-u" in a for a in told["%1"]), told["%1"])


if __name__ == "__main__":                        # pragma: no cover
    unittest.main()
