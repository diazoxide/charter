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
import subprocess
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

from charter import commands_frame, config, instance, statusline, tui
from charter.frame import gather, panel, slots

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

    def test_a_plane_spelled_with_slots_has_no_per_pane_style(self):
        """Per-pane style is written in `[[frame.component]]`; a plane that has not
        written one gets exactly the frame it had."""
        f = instance.frame_of({"frame": {"slots": ["top", "repos"]}})
        self.assertEqual(f["components"], [])
        for name in ("top", "repos", "right"):
            with self.subTest(name=name):
                self.assertEqual(instance.component_style(f, name),
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
        self.assertEqual([a[-1] for a in argvs],
                         [v for _n, v in instance.FRAME_CHROME["dark"]])

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
                         [v for _n, v in instance.FRAME_CHROME["dark"]])
        for a in argvs:
            self.assertNotIn("#", a[-1])

    def test_no_operator_string_reaches_tmux(self):
        """The property, asked of the whole vocabulary: every value in every argv is one
        of charter's own constants. Not "the hostile one is refused" — that is a spelling
        — but "nothing charter did not write can appear here"."""
        ours = {v for pairs in instance.FRAME_PANE_BG.values() for _n, v in pairs}
        ours |= {v for pairs in instance.FRAME_CHROME.values() for _n, v in pairs}
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

    def test_inset_rows_is_the_identity_at_no_pad(self):
        """`" " * 0` is `""` and the join is the split's inverse, so a pane with no pad
        gets back the object's own value unchanged — trailing newline included."""
        with mock.patch.dict(config.FRAME, _arrangement()), _pane(120):
            for text in ("", "a", "a\nb", "a\n", "\n\n"):
                with self.subTest(text=text):
                    self.assertEqual(slots.inset_rows(text, "repos"), text)

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

    def test_a_pane_with_no_columns_at_all_does_not_raise(self):
        """`os.get_terminal_size` can report 0 for a pane being torn down, and `pad_of`
        does arithmetic on it before anything else does."""
        for cols in (0, 1, 2):
            with self.subTest(cols=cols):
                self.assertEqual(self._pad_of("repos", cols=cols, pad=2), 0)


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
        self.assertEqual(self._style(a), "bg=black")
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


class TheFrameDocumentsTheKeys(unittest.TestCase):
    """`docs/frame.md` is where an operator reads what they may write. A key that exists
    and is not documented is a key nobody finds."""

    def test_both_keys_are_in_the_operator_documentation(self):
        text = (Path(__file__).resolve().parents[1] / "docs" / "frame.md").read_text()
        for needle in ('bg = "brightblack"', "pad = 1"):
            with self.subTest(needle=needle):
                self.assertIn(needle, text)


if __name__ == "__main__":                        # pragma: no cover
    unittest.main()
