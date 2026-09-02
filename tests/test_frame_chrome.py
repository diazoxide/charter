"""The frame's paint primitive, and the promises it is built on.

Nothing in this file is visible in a frame. It exists so the visible half can be
believed: `frame/chrome.py` builds on a `tui` guarantee that nothing tested in either
direction, on a pane width that has to be the one the renderer measured, and on a
clear-screen that did not reset the attributes it clears with.

**The `tui._finish` pin lives here rather than in `tests/test_tui.py`** — beside the
module that depends on it, and named for the dependency. `test_tui.py` covers
`term_width`; `test_tui_control_chars.py` covers `width`/`sanitize`/`truncate`/`pad`;
the only file that mentions `_HIDDEN_TRAIL` at all classifies it in a substitution
inventory. So the strip was a documented promise with nothing checking it, in either
direction, on the day `chrome.fill` started depending on it.
"""

from __future__ import annotations

import io
import os
import sys
import unittest
from contextlib import redirect_stdout
from unittest import mock

from charter import statusline, tui
from charter.frame import chrome, panel, slots

from tests._isolation import PersonaIso


class AFinishedLineCarriesNoTrailingPaint(unittest.TestCase):
    """`tui._finish`, pinned in BOTH directions — the promise `chrome.fill` inverts.

    `tui`'s module docstring says "Rendered lines also never carry trailing whitespace,
    even when it hides behind trailing colour escapes", and until this class nothing
    asserted it. It is not a detail: `tui` also renders the status line, which writes
    into a line it does not own and must never leave painted cells trailing across
    somebody else's prompt, and a line with no trailing whitespace is what keeps a copy
    out of the frame clean.

    Both directions, because a one-sided test passes on a `_finish` that strips
    everything: the second half asserts a line that has no trailing whitespace comes
    back BYTE-IDENTICAL, escapes and all.
    """

    def test_a_pad_inside_the_style_span_does_not_survive_a_node(self):
        """The exact measurement `chrome.py` is built around: 20 cells in, 8 out."""
        row = tui.Text("\x1b[36m charter            \x1b[0m").render(40)[0]
        self.assertEqual(row, "\x1b[36m charter\x1b[0m")
        self.assertEqual(tui.width(row), 8)

    def test_a_pad_outside_the_style_span_does_not_survive_either(self):
        """The other spelling of the same 20-cell fill, so a `_finish` that stripped
        only plain trailing spaces — or only the hidden kind — is red for one of them."""
        row = tui.Text("\x1b[36m charter\x1b[0m            ").render(40)[0]
        self.assertEqual(row, "\x1b[36m charter\x1b[0m")

    def test_whitespace_behind_several_escapes_goes_too(self):
        """`_HIDDEN_TRAIL` loops rather than matching once: `' \\x1b[0m \\x1b[2m'` hides a
        space behind an escape behind a space behind an escape, and one pass leaves the
        inner one painted."""
        self.assertEqual(tui.Text("x \x1b[0m \x1b[2m").render(40)[0], "x\x1b[0m\x1b[2m")

    def test_a_row_node_strips_a_fixed_width_cells_own_padding(self):
        """`tui.Cell(markup, 20)` pads to 20 and `_finish` takes it straight back off —
        which is why a fill cannot be composed as a wide cell and why `chrome.fill` takes
        an already-finished row instead."""
        row = tui.Row(tui.Cell("\x1b[36m charter", 20)).render(40)[0]
        self.assertEqual(tui.width(row), 8)

    def test_a_line_with_nothing_trailing_comes_back_byte_identical(self):
        """The other direction. Without this, `_finish` could strip every escape and the
        four assertions above would all still pass."""
        for line in ("\x1b[36m charter\x1b[0m", "plain text", "\x1b[7ma b\x1b[0m", ""):
            with self.subTest(line=line):
                self.assertEqual(tui.Text(line).render(40)[0], line)


class WhatCountsAsAResetIsTheNumber(unittest.TestCase):
    """`chrome.resets_everything` — the sixth instance of #547/#558/#537/#498/#577/#594,
    caught inside the fix for the defect it enables rather than after it shipped.

    "An SGR that resets everything" is a parameter list containing a parameter whose
    NUMERIC VALUE is zero. An empty parameter is zero and leading zeros are legal, so
    `\\x1b[00m` and `\\x1b[1;00m` reset everything and a string test (`p in ("", "0")`)
    misses both. The spellings below are the table, and the two marked MISSED are why
    this class exists.
    """

    def test_every_spelling_of_a_full_reset_is_one(self):
        for params in ("0", "", "00", "1;00", "0;1", "000000", ";1", "22;0"):
            with self.subTest(params=params):
                self.assertTrue(chrome.resets_everything(params),
                                f"\\x1b[{params}m resets everything and was not read as one")

    def test_a_list_with_no_zero_in_it_is_not_a_full_reset(self):
        for params in ("2", "1", "22;39", "7", "27", "38;5;236", "10"):
            with self.subTest(params=params):
                self.assertFalse(chrome.resets_everything(params))

    def test_reverse_off_on_its_own_is_not_a_full_reset(self):
        """SGR 27 turns reverse off and touches nothing else, so it is not the same
        question — `_split_at_close` asks whether a trailing escape leaves any span open,
        and `\\x1b[27m` leaves a colour open."""
        self.assertFalse(chrome.resets_everything("27"))
        self.assertTrue(chrome.cancels_reverse("27"))

    def test_what_cancels_reverse_is_zero_or_twenty_seven_and_nothing_else(self):
        for params in ("0", "", "27", "1;27", "27;1", "0027", "22;0"):
            with self.subTest(params=params):
                self.assertTrue(chrome.cancels_reverse(params))
        for params in ("7", "2", "22;39", "270", "2;7"):
            with self.subTest(params=params):
                self.assertFalse(chrome.cancels_reverse(params))

    def test_the_string_test_this_replaces_would_miss_two_real_spellings(self):
        """The control. Every assertion above is about a function this file also wrote,
        so the check is proved against the implementation it replaced: a reader can see
        that the two tables disagree, and on which rows."""
        def string_test(params: str) -> bool:      # the version that shipped nowhere
            return params in ("", "0")
        missed = [p for p in ("0", "", "00", "1;00")
                  if chrome.resets_everything(p) and not string_test(p)]
        self.assertEqual(missed, ["00", "1;00"],
                         "the numeric test must be strictly stronger than the string one")


class AFillReachesTheLastColumnAndStops(unittest.TestCase):
    """`chrome.fill` — exactly the pane's width, never one more.

    Measured in a real 20-column tmux pane: exactly W is safe (the deferred-wrap state is
    resolved by the following newline and produces no blank row), and **W+1 shears the
    pane** — the fill wraps one cell onto the next row and every row below it shifts
    down. That is #553 arriving through a new door, so the clamp is the point of the
    function and not a safety net on it.
    """

    def test_a_short_row_is_padded_to_exactly_the_width(self):
        out = chrome.fill("\x1b[36m charter\x1b[0m", 20)
        self.assertEqual(tui.width(out), 20)
        self.assertEqual(out, "\x1b[36m charter            \x1b[0m")

    def test_the_pad_goes_inside_a_span_the_row_closes(self):
        """A pad appended after the reset is a pad the terminal paints in the default
        background — a fill with a gap in it, which is what a caller would see and not be
        able to explain."""
        out = chrome.fill("\x1b[7mabc\x1b[0m", 10)
        self.assertTrue(out.endswith("\x1b[0m"), out)
        self.assertNotIn(" \x1b[0m ", out)
        self.assertEqual(out.index(" " * 7), out.index("abc") + 3)

    def test_the_pad_goes_after_a_span_the_row_leaves_open(self):
        """The other half of the same rule, and the case `reverse` composes: a row ending
        in `\\x1b[0m\\x1b[7m` has reverse ON at its end, so the pad belongs after it."""
        out = chrome.fill("\x1b[7mabc\x1b[0m\x1b[7m", 10)
        self.assertTrue(out.endswith("\x1b[0m\x1b[7m       "), repr(out))

    def test_a_row_that_already_fills_the_pane_is_returned_unchanged(self):
        self.assertEqual(chrome.fill("abcde", 5), "abcde")

    def test_a_row_wider_than_the_pane_is_cut_rather_than_wrapped(self):
        """**Which refusal fired** is asserted, not merely the width: the ellipsis is the
        clamp's own signature, so this cannot pass on a `fill` that happened to return a
        short row for some other reason."""
        out = chrome.fill("x" * 30, 20)
        self.assertEqual(tui.width(out), 20)
        self.assertTrue(out.endswith(tui.ELLIPSIS),
                        f"the row was not cut by the clamp: {out!r}")

    def test_a_pane_with_no_columns_at_all_gets_nothing(self):
        """The other refusal, asserted separately — two guards in sequence mask each
        other, and `width <= 0` is reachable from a real `tui.term_width` floor.

        The refusal that fires is `tui.truncate`'s, and `fill` deliberately does not have
        a second one: the mutation table for this branch found an `if width <= 0` here
        SURVIVING, because `truncate` had already answered and the guard could not change
        an outcome. It was deleted rather than pinned. `reverse` keeps its own, which the
        class below asserts is live — the two are not the same line twice."""
        for w in (0, -1):
            with self.subTest(width=w):
                self.assertEqual(chrome.fill("charter", w), "")

    def test_a_filled_row_survives_a_truncate_at_the_pane_width(self):
        """`panel._write` does not re-measure, but `Registry._fit` clips a foreign row —
        so a filled row must be a fixed point of the clamp it may still meet."""
        out = chrome.fill("\x1b[36m charter\x1b[0m", 22)
        self.assertEqual(tui.truncate(out, 22), out)

    def test_a_wide_glyph_is_measured_in_cells_not_characters(self):
        """`tui.width`, never `len`: `⚡` is East-Asian Wide, so a fill counted in
        characters comes out one cell over — which is the W+1 that shears the pane."""
        out = chrome.fill("⚡2", 10)
        self.assertEqual(tui.width(out), 10)
        self.assertEqual(len(out), 9)


class AHighlightedRowIsHighlightedToItsLastColumn(unittest.TestCase):
    """`chrome.reverse` — the element that most makes the frame read as an application,
    and the one with a defect that only appears once it is built.

    A reverse row cancels itself at the first SGR inside it that resets everything, and
    charter's rows carry one after every coloured span. The naive wrapper highlights two
    words of a persona row and leaves the rest plain.
    """

    #: The real sidebar row, as `statusline._persona_chip_cells` composes it.
    ROW = "\x1b[35m▸ \x1b[1msteward\x1b[0m   \x1b[32m✎47\x1b[0m"

    def _reverse_runs(self, out: str) -> list[int]:
        """The visible columns *out* is drawn in reverse video, walked as a terminal
        would walk it: SGR 7 turns it on, a full reset or SGR 27 turns it off."""
        on, col, cols, i = False, 0, [], 0
        while i < len(out):
            m = chrome._SGR.match(out, i)
            if m:
                for p in m.group(1).split(";"):
                    v = int(p or "0")
                    on = True if v == 7 else False if v in (0, 27) else on
                i = m.end()
                continue
            if on:
                cols.append(col)
            col += tui.width(out[i])
            i += 1
        return cols

    def test_a_row_carrying_a_plain_reset_stays_highlighted_to_the_end(self):
        out = chrome.reverse(self.ROW, 40)
        self.assertEqual(tui.width(out), 40)
        self.assertEqual(self._reverse_runs(out), list(range(40)))

    def test_a_row_carrying_a_leading_zero_reset_stays_highlighted_too(self):
        """`\\x1b[00m`. A `p in ("", "0")` implementation passes every test written
        against `\\x1b[0m` and fails this one — which is why it is written first."""
        out = chrome.reverse("a\x1b[00mb", 12)
        self.assertEqual(tui.width(out), 12)
        self.assertEqual(self._reverse_runs(out), list(range(12)))

    def test_a_row_carrying_a_reset_with_another_parameter_stays_highlighted_too(self):
        """`\\x1b[1;00m` — the second spelling the string test misses."""
        out = chrome.reverse("a\x1b[1;00mb", 12)
        self.assertEqual(self._reverse_runs(out), list(range(12)))

    def test_the_naive_wrapper_would_fail_both_of_those(self):
        """The live control. Every assertion above is about reverse REACHING the end, and
        a broken `_reverse_runs` would report that just as happily — so the walker is
        proved against the wrapper this function replaced."""
        naive = "\x1b[7m" + self.ROW + " " * 25 + "\x1b[27m"
        self.assertEqual(max(self._reverse_runs(naive)) + 1, 9,
                         "the naive wrapper is supposed to stop at the first reset")

    def test_a_row_carrying_a_bare_reverse_off_stays_highlighted_too(self):
        """`\\x1b[27m` turns reverse off and nothing else, so a re-assertion keyed on
        "resets everything" walks straight past it and the row goes plain from there.
        Charter writes none today; a provider's component is ordinary Python."""
        out = chrome.reverse("a\x1b[27mb", 12)
        self.assertEqual(self._reverse_runs(out), list(range(12)))

    def test_reverse_is_re_asserted_only_where_it_was_cancelled(self):
        """An escape that changes nothing is bytes written into a pane on every repaint
        for no change on screen. Re-asserting after EVERY SGR keeps the row highlighted
        just as well, which is why the assertions above cannot tell the two apart — this
        counts. The fixture carries two escapes that cancel reverse (`\\x1b[0m`,
        `\\x1b[27m`) and two that do not (`\\x1b[35m`, `\\x1b[2m`), so the answer is the
        one leading assertion plus two."""
        out = chrome.reverse("\x1b[35ma\x1b[0mb\x1b[2mc\x1b[27md", 20)
        self.assertEqual(out.count("\x1b[7m"), 3, repr(out))
        self.assertEqual(self._reverse_runs(out), list(range(20)),
                         "and it is still highlighted the whole way")

    def test_a_row_with_no_escapes_at_all_is_highlighted_whole(self):
        out = chrome.reverse("steward", 20)
        self.assertEqual(self._reverse_runs(out), list(range(20)))

    def test_a_row_wider_than_the_pane_is_cut_and_still_highlighted_whole(self):
        out = chrome.reverse("s" * 40, 20)
        self.assertEqual(tui.width(out), 20)
        self.assertEqual(self._reverse_runs(out), list(range(20)))

    def test_a_highlighted_row_ends_with_every_attribute_off(self):
        """A row that left an attribute open would carry it past the highlight into the
        rest of the pane. SGR 27 alone would not: it turns reverse off and nothing else."""
        self.assertTrue(chrome.reverse("\x1b[1msteward", 20).endswith(tui.RESET))

    def test_a_pane_with_no_columns_at_all_gets_nothing(self):
        """`reverse`'s own refusal, and it is a LIVE one: without it a zero-width pane
        still gets `\\x1b[0m` written into it, because the closing reset is appended after
        `fill` has already answered. Deleting the line makes this red — which is what
        `fill`'s equivalent line did not do, and why `fill` no longer has one."""
        for w in (0, -1):
            with self.subTest(width=w):
                self.assertEqual(chrome.reverse("steward", w), "")

    def test_a_highlighted_row_survives_a_truncate_at_the_pane_width(self):
        out = chrome.reverse(self.ROW, 22)
        self.assertEqual(tui.truncate(out, 22), out)


class ColourIsARequestTheOperatorCanRefuse(unittest.TestCase):
    """`chrome.colour_ok` — `NO_COLOR` by PRESENCE, and one function asking it.

    `NO_COLOR` appeared nowhere in `charter/` before this: `util._USE_COLOR` gates
    `util.info/ok/warn/err` and nothing else, and the frame renderers coloured
    unconditionally — measured, `panel.run` with stdout a `StringIO` wrote a
    clear-screen and full SGR into it.

    The property is `os.environ.get("NO_COLOR") is not None`. `== "1"` is the
    spelling-not-property mistake in the one file that is about that mistake, and it
    breaks first on `NO_COLOR=` — which is what a shell that exports the variable with no
    value has set.

    **This is charter's rule and it used to be cited as no-color.org's.** That page has
    said "present *and not an empty string*" since `jcs/no_color` `99f90e27` (2022-06-27),
    so the citation was a quote of the sentence that commit replaced, and the one input it
    disagrees about is the `NO_COLOR=""` this class asserts on first. The behaviour is
    unchanged and the argument for it is in `chrome.no_colour`; what these cases pin is
    charter's rule, which is the only thing they were ever able to pin.
    """

    def _tty(self, answer: bool):
        return mock.patch.object(sys.stdout, "isatty", return_value=answer,
                                 create=True)

    def test_an_empty_no_color_still_means_no_colour(self):
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}, clear=True), self._tty(True):
            self.assertFalse(chrome.colour_ok())

    def test_a_falsey_looking_no_color_still_means_no_colour(self):
        """`NO_COLOR=0` is set, so it means no colour. A value test reads it as "off",
        which is the opposite of what the operator wrote."""
        with mock.patch.dict(os.environ, {"NO_COLOR": "0"}, clear=True), self._tty(True):
            self.assertFalse(chrome.colour_ok())

    def test_any_other_value_means_no_colour_too(self):
        for value in ("1", "yes", "true", "  "):
            with self.subTest(value=value), \
                 mock.patch.dict(os.environ, {"NO_COLOR": value}, clear=True), \
                 self._tty(True):
                self.assertFalse(chrome.colour_ok())

    def test_an_unset_no_color_on_a_tty_colours(self):
        with mock.patch.dict(os.environ, {}, clear=True), self._tty(True):
            self.assertTrue(chrome.colour_ok())

    def test_a_stdout_that_cannot_answer_does_not_colour(self):
        """A closed stdout raises `ValueError` from `isatty()` and a replacement object
        without the method raises `AttributeError` — both are real, and a frame that
        cannot tell does not colour, which is the direction `NO_COLOR` already points.

        Pinned because the deletion sweep found the catch unreached: narrowing it to
        `ZeroDivisionError` left the whole suite green, which is a guard with nothing
        behind it."""
        class _Closed:
            def isatty(self):
                raise ValueError("I/O operation on closed file")

        class _Silent:
            pass

        for stdout in (_Closed(), _Silent()):
            with self.subTest(stdout=type(stdout).__name__), \
                 mock.patch.dict(os.environ, {}, clear=True), \
                 mock.patch.object(sys, "stdout", stdout):
                self.assertFalse(chrome.colour_ok())

    def test_a_redirected_panel_does_not_colour_a_file(self):
        """`charter panel top --session x > /tmp/log`, the case `panel.py`'s own
        docstring documents. It is the only thing `isatty` catches in a real frame — a
        panel's stdout IS its pane — and it is written down here so nobody reads it as
        the mechanism."""
        with mock.patch.dict(os.environ, {}, clear=True), self._tty(False):
            self.assertFalse(chrome.colour_ok())


class AClearScreenClearsWithWhateverIsSet(PersonaIso, unittest.TestCase):
    """`panel._write` prefixes a reset — §6.3, measured in a real pane.

    A renderer that leaves a background set makes the NEXT repaint's `\\x1b[2J` fill the
    whole pane with it, because erase uses the current attributes. Constraint 4 still
    holds (it costs that pane and no other) but the pane stays wrong until something
    resets it, and nothing did. Measured, two paints in one 20-column pane::

        after the next '\\x1b[H\\x1b[2J' + 'second paint':
          row 0: '\\x1b[48;5;196msecond paint        '   <- the leak survived the clear
          row 1: '                    '                 <- and filled every other row

    The reset goes BEFORE the cursor-home, so `split("\\x1b[2J", 1)[1]` still answers the
    content — four call sites structurally depend on that
    (`tests/test_frame_panel.py:129,172,193`, `tests/test_component_id_is_the_currency.py:110`).
    """

    def _paint(self, text: str) -> str:
        buf = io.StringIO()
        with redirect_stdout(buf), \
             mock.patch.object(sys.stdout, "isatty", return_value=True, create=True), \
             mock.patch("charter.frame.panel._rows", return_value=4):
            panel._write(text)
        return buf.getvalue()

    def test_a_paint_resets_before_it_clears(self):
        out = self._paint("second paint")
        self.assertTrue(out.startswith("\x1b[m\x1b[H\x1b[2J"), repr(out[:20]))

    def test_the_reset_is_a_full_one_and_not_only_a_background_reset(self):
        """`\\x1b[49m` would leave a foreground, a bold or a reverse from the leaking
        paint in place, and the erase would be clean while every glyph after it was not."""
        head = self._paint("x").split("\x1b[H", 1)[0]
        self.assertTrue(chrome.resets_everything(head[2:-1]), repr(head))

    def test_the_content_is_still_reachable_by_splitting_on_the_clear(self):
        """The compatibility half, asserted rather than assumed: the reset is before the
        cursor-home precisely so this keeps working."""
        self.assertEqual(self._paint("second paint").split("\x1b[2J", 1)[1],
                         "second paint")


class TheFrameEmitsNoColourWhenTheOperatorRefusedIt(PersonaIso, unittest.TestCase):
    """`NO_COLOR` reaches the pane, not merely the helper.

    Asked in `panel._write`, the one place anything reaches a pane's screen — so a
    component charter did not write is covered by the same answer as a renderer charter
    did, and neither has to remember to ask.
    """

    def _paint(self, env: dict, *, tty: bool = True) -> str:
        buf = io.StringIO()
        with mock.patch.dict(os.environ, env, clear=True), redirect_stdout(buf), \
             mock.patch.object(sys.stdout, "isatty", return_value=tty, create=True), \
             mock.patch("charter.frame.panel._rows", return_value=4):
            panel._write("\x1b[35m▸ \x1b[1msteward\x1b[0m \x1b[32m✎47\x1b[0m")
        return buf.getvalue()

    def test_no_color_leaves_the_pane_with_no_sgr_at_all(self):
        out = self._paint({"NO_COLOR": ""})
        self.assertNotIn("\x1b[", out.split("\x1b[2J", 1)[1])
        self.assertIn("steward", out, "the content itself must survive")

    def test_no_color_drops_the_reset_prefix_too(self):
        """There is nothing to reset when nothing is painted, and "no SGR from the frame
        at all" is the promise — a reset is an SGR."""
        self.assertTrue(self._paint({"NO_COLOR": "1"}).startswith("\x1b[H\x1b[2J"))

    def test_no_color_leaves_no_invisible_pad_behind_either(self):
        """`chrome.fill`'s pad is only meaningful under paint. Stripped with it, so a
        `NO_COLOR` pane copies out as the text it shows."""
        buf = io.StringIO()
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}, clear=True), \
             redirect_stdout(buf), \
             mock.patch.object(sys.stdout, "isatty", return_value=True, create=True), \
             mock.patch("charter.frame.panel._rows", return_value=4):
            panel._write(chrome.reverse("\x1b[1msteward\x1b[0m", 30))
        self.assertEqual(buf.getvalue().split("\x1b[2J", 1)[1], "steward")

    def test_an_ordinary_pane_still_gets_its_colour(self):
        """The control: every assertion above is a negative, and a `_write` that emitted
        nothing would satisfy all of them."""
        self.assertIn("\x1b[35m", self._paint({}))

    def test_the_clear_screen_survives_either_way(self):
        """`\\x1b[H\\x1b[2J` is not colour and is not charter's markup — it is the thing
        that makes a repaint a repaint. A `NO_COLOR` implementation written as
        `tui.strip_ansi(whole_output)` would delete it, because `sanitize` drops every
        CSI that is not SGR."""
        for env in ({"NO_COLOR": ""}, {}):
            with self.subTest(env=env):
                self.assertIn("\x1b[H\x1b[2J", self._paint(env))


class TheFillUsesTheWidthTheRendererMeasured(PersonaIso, unittest.TestCase):
    """One measurement, not two — the mutation `slots._width()` exists to refuse.

    A panel process inherits the LAUNCHING shell's `$COLUMNS` whole (measured: a
    22-column pane whose launcher had exported `COLUMNS=200`). A fill computed against
    that number in a 22-column pane is 178 cells over, and every row wraps four times.
    """

    def _render(self, cols: int) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, 24))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return slots.render("right", "f-1")

    def test_a_painted_sidebar_row_never_exceeds_the_panes_own_columns(self):
        self.make_persona("a-persona-with-quite-a-long-name")
        with mock.patch.dict(os.environ, {"COLUMNS": "200"}):
            out = self._render(22)
        for line in out.split("\n"):
            with self.subTest(line=line):
                self.assertLessEqual(tui.width(line), 22, repr(line))

    def test_a_painted_row_reaches_the_panes_own_last_column(self):
        """The other direction, so the test above cannot be satisfied by a panel that
        stopped painting. The selected row is the one that fills."""
        self.make_persona("alice")
        with mock.patch("charter.persona.resolve_active", return_value="alice"):
            out = self._render(22)
        painted = [ln for ln in out.split("\n") if "\x1b[7m" in ln]
        self.assertTrue(painted, f"no row was painted at all: {out!r}")
        for line in painted:
            self.assertEqual(tui.width(line), 22, repr(line))


if __name__ == "__main__":
    unittest.main()
