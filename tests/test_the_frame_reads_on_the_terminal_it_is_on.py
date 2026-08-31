"""What a frame looks like on a ground charter did not choose — the three defects that
only appear once you run it on the other kind of terminal.

**Every one of these was written on a dark terminal and is wrong on a light one, or the
mirror of that.** That is the whole reason the cluster exists, so the assertions here are
written as PAIRS wherever a pair is possible: what the row says about the operator's
foreground and what it says about their background, asked separately, so a fix verified on
one ground cannot pass as a fix.

* **#737** — ``chrome`` set a background and no foreground, so one of its two words made
  every panel's text the colour of the surface it was drawn on.
* **#736** — a reverse-video row set an absolute foreground INSIDE the inverted run, which
  is the one place a sixteen-colour name stops meaning what the operator's theme says.
* **#750** — with the shipped ``chrome = "off"`` nothing said which pane the keyboard was
  in, and `docs/frame.md` described a cue that only exists once a surface is set.

The real-tmux half of #750 lives in `test_a_planes_frame_really_reads_that_way.py`, beside
the other measurements taken off an attached client's wire; what is here is charter's own
answer, which is what a machine with no tmux can still check.
"""

from __future__ import annotations

import os
import re
import unittest
from unittest import mock

from charter import commands_frame, config, instance, statusline as sl, tui
from charter.frame import chrome, slots
from tests._isolation import PersonaIso

#: One SGR escape, parameters captured — this file's own reading, so a change to
#: `chrome._SGR` cannot quietly change what these tests measure.
_SGR = re.compile(r"\x1b\[([0-9;]*)m")

#: Every SGR parameter that names one of the sixteen as a FOREGROUND, and every one that
#: names a background. Spelled here as the ECMA-48/aixterm ranges rather than imported from
#: `chrome`, because the implementation's own set is the thing under test.
_FG = frozenset({*range(30, 40), *range(90, 98)})
_BG = frozenset({*range(40, 50), *range(100, 108)})


def _params(row: str) -> list[int]:
    """Every SGR parameter in *row*, as numbers — an empty parameter is the zero it means."""
    return [int(p or "0") for m in _SGR.finditer(row) for p in m.group(1).split(";")]


class TheChromeTableAndItsForegroundsAreOneVocabulary(unittest.TestCase):
    """#737 — a surface word without a paired foreground is half a recipe.

    The two tables are keyed identically and asserted so, for `chrome.served_params`' own
    reason one module over: a second list of the words is exactly how a third surface comes
    to ship with a background and no foreground, which is the defect rather than a risk of
    it.
    """

    def test_every_chrome_word_says_what_goes_on_it(self):
        self.assertEqual(set(instance.FRAME_CHROME), set(instance.FRAME_CHROME_FG),
                         "a surface word and its foreground must arrive together")

    def test_the_two_surfaces_charter_ships_carry_opposite_poles(self):
        """`bg=black` wants `fg=white` and `bg=white` wants `fg=black`, and this is the only
        pairing in charter that is a measurement rather than a guess: the two are the POLES
        of the sixteen, so a theme is not free to render them the same way round."""
        self.assertEqual(instance.FRAME_CHROME_FG["dark"], "fg=white")
        self.assertEqual(instance.FRAME_CHROME_FG["light"], "fg=black")

    def test_off_pairs_with_nothing_at_all(self):
        """`off` is the absence of a surface, so there is no background for a foreground to
        go with — and the empty clause is what keeps a plane that said nothing emitting the
        options it emitted before this table existed."""
        self.assertEqual(instance.FRAME_CHROME_FG["off"], "")

    def test_a_word_charter_does_not_know_leaves_through_neither_half(self):
        """`chrome_fg`'s containment is `text_fg`'s: what comes back is charter's own
        constant, so a committed word indexes nothing."""
        for word in ("solarized", "", None, 7, ["light"], {"a": 1}):
            with self.subTest(word=word):
                self.assertEqual(instance.chrome_fg(word), "")

    def test_the_paired_foreground_is_a_palette_name_and_never_an_index(self):
        """§3.2 said about the new table — `TheFrameNamesColoursAndNeverIndexes` names
        `FRAME_CHROME` and `FRAME_PANE_BG`, and this one has to be inside the same rule
        rather than an exception to it."""
        known = {*instance.FRAME_PANE_COLOURS,
                 *(f"bright{n}" for n in instance.FRAME_PANE_COLOURS), "default"}
        for word, clause in instance.FRAME_CHROME_FG.items():
            with self.subTest(word=word):
                if clause:
                    self.assertTrue(clause.startswith("fg="), clause)
                    self.assertIn(clause.removeprefix("fg="), known, clause)


class ASurfaceCarriesTheTextThatGoesOnIt(unittest.TestCase):
    """#737 — the pairing where it reaches tmux, which is the only place it counts."""

    #: A plane that has said nothing about its text.
    QUIET = instance.SHIPPED_LOOK

    def test_the_light_surface_stops_drawing_light_text_on_itself(self):
        """The defect, stated as the operator meets it: `chrome = "light"` on a dark
        terminal painted `bg=white` and left every uncoloured cell in the terminal's own
        foreground, which on that theme is white."""
        for name, value in instance.surface_options(None, "light", self.QUIET):
            with self.subTest(option=name):
                self.assertIn("fg=black", value, f"{name} = {value!r}")
                self.assertIn("bg=", value, f"{name} = {value!r}")

    def test_the_dark_surface_stops_drawing_dark_text_on_itself(self):
        """The mirror, which is the half a developer on a dark terminal never sees fail."""
        for name, value in instance.surface_options(None, "dark", self.QUIET):
            with self.subTest(option=name):
                self.assertIn("fg=white", value, f"{name} = {value!r}")

    def test_both_style_options_carry_the_same_foreground(self):
        """A foreground that moved with focus would say which pane is live twice, and
        `FRAME_PANE_FG`'s own docstring argues why it must not: text that changed colour
        from pane to pane stops reading as one document. The background pair is what says
        it."""
        for chrome_word in ("dark", "light"):
            with self.subTest(chrome=chrome_word):
                got = {v.split(",")[0]
                       for _n, v in instance.surface_options(None, chrome_word, self.QUIET)}
                self.assertEqual(len(got), 1, got)

    def test_the_planes_own_text_word_still_wins(self):
        """The pairing is charter's default for its own recipe, not a second opinion over a
        plane that said what its frame's text is."""
        look = instance.Look("hidden", "blue", True)
        for name, value in instance.surface_options(None, "light", look):
            with self.subTest(option=name):
                self.assertIn("fg=blue", value, f"{name} = {value!r}")
                self.assertNotIn("fg=black", value, f"{name} = {value!r}")

    def test_the_style_carries_exactly_one_foreground(self):
        """**`TheStyleCarriesExactlyOneForeground`.** A value with two `fg=` clauses is a
        frame whose colour depends on a tmux parsing rule nobody wrote down — so the two
        are resolved to one clause rather than appended in an order that leaves last-wins
        to decide. Over every pairing of the two vocabularies."""
        words = [*instance.FRAME_PANE_FG]
        for chrome_word in instance.FRAME_CHROME:
            for text in words:
                look = instance.Look("hidden", text, True)
                for bg in (None, "blue", "default"):
                    for name, value in instance.surface_options(bg, chrome_word, look):
                        with self.subTest(chrome=chrome_word, text=text, bg=bg):
                            self.assertLessEqual(value.count("fg="), 1,
                                                 f"{name} = {value!r}")

    def test_a_component_that_named_its_own_background_gets_no_foreground_it_did_not_ask_for(self):
        """The line the pairing draws, and the reason it is not seventeen words wide.
        `bg = "blue"` is the operator's word out of their own palette; charter cannot see
        what their blue looks like, so it does not decide what goes on it. `[frame] text`
        is how that pane is told."""
        for name, value in instance.surface_options("blue", "light", self.QUIET):
            with self.subTest(option=name):
                self.assertNotIn("fg=", value, f"{name} = {value!r}")

    def test_a_frame_with_no_surface_at_all_is_byte_identical(self):
        """`chrome = "off"` and no `bg`: the shipped frame, which must not have moved.
        `_surface_argvs` issues no command at all for an empty answer, so this is the
        assertion that the default did not gain a pane option."""
        self.assertEqual(instance.surface_options(None, "off", self.QUIET), ())

    def test_no_operator_string_reaches_the_foreground_half(self):
        """The containment, asked the way `pane_bg_options`' own is: a `chrome` value from
        a committed `charter.toml` is a KEY, and what leaves is charter's constant."""
        for hostile in ("light,bg=#{?#{==:1,1},colour196,colour46}", "#(touch /tmp/x)",
                        "light;bg=red"):
            with self.subTest(hostile=hostile):
                for _n, value in instance.surface_options(None, hostile, self.QUIET):
                    self.assertNotIn("#", value)
                self.assertEqual(instance.chrome_fg(hostile), "")


class TheRuleStillReadsABareBackgroundClause(unittest.TestCase):
    """#737's blast radius, pinned rather than hoped for.

    `instance.rule_style`'s `hidden` arm does `surface.removeprefix("bg=")` and
    `TheSurfaceIsAlwaysABareBackgroundClause` pins that over both tables. The paired
    foreground is therefore a SEPARATE table rather than an `fg=` appended to
    `FRAME_CHROME`'s values: an `fg` inside a `window-style` entry would reach that
    `removeprefix` and compose a rule with two foregrounds in it.
    """

    def test_every_chrome_surface_is_still_exactly_a_background_clause(self):
        for word, pairs in instance.FRAME_CHROME.items():
            for name, value in pairs:
                with self.subTest(word=word, option=name):
                    self.assertTrue(value.startswith("bg="), value)
                    self.assertNotIn("fg=", value)

    def test_the_rule_over_a_paired_surface_still_carries_one_foreground(self):
        got = instance.pane_border_options(None, "light", commands_frame._CHROME_FG,
                                           instance.SHIPPED_LOOK)
        self.assertTrue(got)
        for name, value in got:
            with self.subTest(option=name):
                self.assertEqual(value.count("fg="), 1, f"{name} = {value!r}")


class TheRowYouAreOnNamesNoColourAtAll(unittest.TestCase):
    """#736 — reverse video is only theme-independent while the run sets no colour.

    `docs/frame.md` argued the highlight is right on every scheme *precisely because it
    names nothing*, and both surfaces that used it named something. These are the two rows
    read off the operator's own live frame with `capture-pane -e`.
    """

    #: The sidebar's active persona row, as `statusline._persona_chip_cells` composes it.
    SIDEBAR = "\x1b[35m▸ \x1b[1msteward\x1b[0m   \x1b[32m✎47\x1b[0m"

    #: The repo table's selected row — cyan for the name, **yellow** for a dirty branch.
    REPOS = ("  \x1b[2m├─ \x1b[0m\x1b[36mbilling\x1b[39m       "
             "\x1b[33mmain*\x1b[39m  \x1b[32m✓ passed\x1b[39m")

    def test_the_sidebars_chosen_row_carries_no_foreground(self):
        got = chrome.reverse(self.SIDEBAR, 40)
        self.assertFalse([p for p in _params(got) if p in _FG],
                         f"a foreground survived the inversion: {got!r}")

    def test_the_repo_tables_chosen_row_carries_no_foreground(self):
        """The one #736 is titled for: `SGR 33` on the terminal's own foreground is the
        worst pair the sixteen can make, on the row that says which repo you picked."""
        got = chrome.reverse(self.REPOS, 60)
        self.assertFalse([p for p in _params(got) if p in _FG],
                         f"a foreground survived the inversion: {got!r}")

    def test_it_carries_no_background_either(self):
        """Under reverse a background parameter paints the TEXT, so both halves of the pair
        are the same defect — asked separately because a fix that deleted one and not the
        other would pass every assertion above."""
        got = chrome.reverse("\x1b[41m\x1b[7mred bg\x1b[0m", 20)
        self.assertFalse([p for p in _params(got) if p in _BG],
                         f"a background survived the inversion: {got!r}")

    def test_an_extended_colour_is_taken_whole_and_not_a_digit_at_a_time(self):
        """The hazard `_without_dim` names and this one inherits: a pass that filtered the
        parameter list would turn `38;2;255;0;0` into `255;0;0` and hand the terminal an
        SGR nobody wrote. A provider's component is ordinary Python and writes what its
        author's terminal supports."""
        for spelling in ("\x1b[38;2;255;0;0m", "\x1b[48;5;236m", "\x1b[1;38;5;236mx",
                         "\x1b[58;2;1;2;3m"):
            with self.subTest(spelling=spelling):
                got = chrome.reverse(f"a{spelling}b", 12)
                self.assertNotIn("38", got, repr(got))
                self.assertNotIn("48", got, repr(got))
                self.assertNotIn("255", got, repr(got))
                self.assertNotIn("236", got, repr(got))

    def test_a_truncated_colour_introducer_is_taken_as_one_parameter(self):
        """**`ESC[38m` with nothing after it**, which is where the walk runs off the end.

        The three survivors `chrome.undim` was pinned for, asked again of the pass beside
        it: an introducer at the END of the list has no space parameter to read, and a walk
        that reached for one would raise `IndexError` inside a repaint — a panel that dies
        rather than a colour that survives. `tui.sanitize` passes any
        `ESC[<digits and semicolons>m` through, so a provider's row really can carry this.

        `38` is dropped like any other colour and what followed it, if anything, is read as
        the ordinary parameters they are.
        """
        for row, want in (("\x1b[38m", ""),
                          ("\x1b[48m", ""),
                          ("\x1b[58m", ""),
                          ("\x1b[1;38m", "\x1b[1m"),
                          ("\x1b[38;m", "\x1b[m")):
            with self.subTest(row=row):
                got = chrome.reverse(f"a{row}b", 12)
                self.assertIn(f"a{want}", got, repr(got))
                self.assertFalse([p for p in _params(got) if p in _FG or p in _BG],
                                 repr(got))

    def test_a_colour_space_charter_does_not_know_is_taken_as_one_parameter_too(self):
        """The other half of the same walk. `ESC[38;99m` names a colour space that does not
        exist, so there is no run length to consume — the introducer goes and the `99` is
        whatever a terminal makes of it, which is not charter's to rewrite. A pass with no
        fallback would raise or would swallow parameters it has no reason to.
        """
        for row, want in (("\x1b[38;99m", "\x1b[99m"),
                          ("\x1b[38;4;9m", "\x1b[4;9m"),
                          ("\x1b[58;7;1m", "\x1b[7;1m"),
                          ("\x1b[48;0m", "\x1b[0m")):
            with self.subTest(row=row):
                got = chrome.reverse(f"a{row}b", 12)
                self.assertIn(f"a{want}", got, repr(got))

    def test_the_run_lengths_the_two_real_colour_spaces_have_are_the_ones_consumed(self):
        """`5;<n>` is three parameters and `2;<r>;<g>;<b>` is five, and off-by-one either
        way leaves a stray digit in the row that a terminal reads as an SGR of its own.
        Asserted as the whole answer rather than as an absence."""
        for row, want in (("\x1b[38;5;236m", ""),
                          ("\x1b[38;5;236;1m", "\x1b[1m"),
                          ("\x1b[38;2;255;0;0m", ""),
                          ("\x1b[38;2;255;0;0;4m", "\x1b[4m"),
                          ("\x1b[1;48;5;236;2m", "\x1b[1;2m")):
            with self.subTest(row=row):
                got = chrome.reverse(f"a{row}b", 12)
                self.assertIn(f"a{want}b", got, repr(got))

    def test_a_colour_spelled_with_a_leading_zero_goes_too(self):
        """The numeric reading, which this file's six predecessors (#547, #558, #537, #498,
        #577, #594) are the reason for: `ESC[032m` is green and a string match for `32`
        finds it, but `ESC[1;032m` is bold AND green and only one of the two goes."""
        got = chrome.reverse("a\x1b[1;032mb", 12)
        self.assertFalse([p for p in _params(got) if p in _FG], repr(got))
        self.assertIn(1, _params(got), f"the bold went with it: {got!r}")

    def test_the_attributes_inside_the_run_are_untouched(self):
        """A deletion of colour and not of everything: bold adds weight and dim reduces it
        relative to whatever ground the cell already has, so neither can be wrong on a
        theme charter cannot see. `_table_row`'s emphasis and the tree's dim are drawn with
        exactly these."""
        got = chrome.reverse("\x1b[1m\x1b[4mA\x1b[2mB", 12)
        for attr in (1, 4, 2):
            with self.subTest(attr=attr):
                self.assertIn(attr, _params(got), repr(got))

    def test_an_escape_that_emptied_is_deleted_and_never_left_as_a_bare_reset(self):
        """The correctness of the whole pass, and the reason `_without_colour` answers
        `None` rather than `""`: `ESC[m` is an omitted parameter, an omitted parameter is
        SGR's default of ZERO, and a `ESC[35m` rewritten to `ESC[m` would turn everything
        off — cancelling the reverse one escape after it was asserted."""
        got = chrome.reverse("a\x1b[35mb", 12)
        self.assertNotIn("\x1b[m", got, repr(got))

    def test_a_colour_channel_that_happens_to_be_zero_is_not_read_as_a_reset(self):
        """**A colour's arguments are not SGR parameters**, and by the time
        `chrome.cancels_reverse` sees a list they are all just numbers. `ESC[38;2;255;0;0m`
        is a 24-bit red foreground whose green and blue channels are zero — read whole it
        says "reverse was cancelled", and charter answers a `ESC[7m` that changes nothing
        on the screen, on every repaint, forever.

        Deleting the colour first is what turns the list into its parameters, so the
        question is asked of what survived. This asserts the byte that is NOT there."""
        for row in ("\x1b[38;2;255;0;0m", "\x1b[38;2;0;0;27m", "\x1b[48;5;0m",
                    "\x1b[38;5;27m"):
            with self.subTest(row=row):
                got = chrome.reverse(f"a{row}b", 12)
                self.assertEqual(got.count("\x1b[7m"), 1,
                                 f"reverse was re-asserted for a colour channel: {got!r}")

    def test_an_escape_that_really_did_reset_still_gets_its_re_assertion(self):
        """The control for the line above, and the reason it is not a licence to stop
        re-asserting: a `0` that is a PARAMETER survives the deletion and is answered."""
        for row in ("\x1b[0m", "\x1b[38;2;255;0;0;0m", "\x1b[38;m", "\x1b[27m"):
            with self.subTest(row=row):
                got = chrome.reverse(f"a{row}b", 12)
                self.assertEqual(got.count("\x1b[7m"), 2,
                                 f"the highlight was not put back: {got!r}")

    def test_a_reset_inside_the_run_still_survives_with_its_re_assertion(self):
        """`0` is not a colour, so the pass leaves it — and `cancels_reverse` is asked of
        the ORIGINAL list, so the re-assertion still lands behind it."""
        got = chrome.reverse("a\x1b[0mb", 12)
        self.assertIn("\x1b[0m\x1b[7m", got, repr(got))

    def test_the_row_is_still_highlighted_to_the_last_column(self):
        """The property the deletion must not cost. Walked as a terminal would."""
        for row, width in ((self.SIDEBAR, 40), (self.REPOS, 60)):
            with self.subTest(width=width):
                got = chrome.reverse(row, width)
                self.assertEqual(tui.width(got), width)
                on = True
                for i, ch in enumerate(got):
                    m = _SGR.match(got, i)
                    if m and m.start() == i:
                        for p in m.group(1).split(";"):
                            v = int(p or "0")
                            on = True if v == 7 else False if v in (0, 27) else on
                self.assertFalse(on, "the row left reverse video open")

    def test_the_row_says_exactly_what_it_said_before(self):
        """A deletion of colour changes no cell's text and no cell's width — every SGR
        costs zero columns. `chrome.plain` is the frame's own reading of "what a terminal
        with no colour would show", and it is unmoved."""
        for row, width in ((self.SIDEBAR, 40), (self.REPOS, 60)):
            with self.subTest(width=width):
                self.assertEqual(chrome.plain(chrome.reverse(row, width)),
                                 chrome.plain(tui.truncate(row, width)))

    def test_a_provider_composing_with_the_selected_recipe_gets_the_same_answer(self):
        """The third caller, which is why the pass is in `reverse` and not at the two call
        sites in `slots.py`. A component's row reaching this is ordinary Python that may
        have written any colour at all."""
        got = chrome.reverse("\x1b[33mprovider\x1b[0m", 20)
        self.assertFalse([p for p in _params(got) if p in _FG], repr(got))


class TheRealRowCharterDrawsCarriesNoColourInside(PersonaIso, unittest.TestCase):
    """#736 measured through the renderer rather than through a fixture.

    A fixture is a row somebody typed, and this whole cluster exists because a row somebody
    typed matched what was drawn on the machine it was typed on. This renders the repo
    table — the surface #736 is titled for — from `gather`-cache-shaped rows and walks the
    line that came back.

    The rows are built here rather than read from the plane this suite is running on
    (#785): a table read live carries whatever the operator's clones happen to be doing,
    and "the dirty branch is yellow" is the case that has to be in the fixture rather than
    hoped for.
    """

    #: Wide enough that no column is dropped — `statusline._LEFT_W` is the floor.
    WIDE = 200

    def _rows(self, name, **kw):
        row = {"name": name, "branch": "main", "dirty": True, "tracked_dirty": False,
               "ahead": 0, "behind": 3, "ci": "success", "change": 4, "sigil": "✎",
               "current": False, "worktree_count": 0}
        row.update(kw)
        return row

    def _table(self, selected):
        data = {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                "repos": [self._rows("alpha"), self._rows("beta", ci="failed")],
                "worktrees": []}
        return slots._table_lines(data, self.WIDE, 6, selected=selected)

    def test_the_unselected_rows_still_carry_their_colours(self):
        """The control, and it is not decoration: "no colour inside the inversion" is
        equally satisfied by a renderer that drew no colour at all, and this fixture is
        exactly the dirty-branch, CI-failing shape #736 measured."""
        rows = [ln.text for ln in self._table(selected="alpha")]
        self.assertTrue([p for p in _params(rows[1]) if p in _FG],
                        f"the fixture drew no colour to delete: {rows[1]!r}")

    def test_the_selected_row_carries_none(self):
        rows = [ln.text for ln in self._table(selected="alpha")]
        self.assertIn("\x1b[7m", rows[0], "the fixture did not highlight anything")
        self.assertFalse([p for p in _params(rows[0]) if p in _FG or p in _BG],
                         f"a colour survived the inversion: {rows[0]!r}")

    def test_the_yellow_branch_is_the_one_that_goes(self):
        """#736's own headline. A dirty branch is `statusline._YELLOW`; inside the reversed
        run that is yellow on the terminal's own foreground."""
        selected = self._table(selected="alpha")[0].text
        self.assertNotIn(33, _params(selected), repr(selected))
        unselected = self._table(selected="beta")[0].text
        self.assertIn(33, _params(unselected),
                      f"the control lost its yellow too: {unselected!r}")

    def test_the_row_still_says_what_it_said(self):
        """Selection is paint and never content — `TheSelectedRowIsDrawnAsSelected`'s own
        rule, re-asked now that the paint is a deletion."""
        self.assertEqual(chrome.plain(self._table(selected="alpha")[0].text),
                         chrome.plain(self._table(selected="beta")[0].text))


class TheFrameSaysWhichPaneIsLive(unittest.TestCase):
    """#750 — with the shipped `chrome = "off"` there was no answer at all.

    Charter's half of it: the option is pinned, and it is pinned to the value that draws a
    cue. What tmux actually puts on the wire for it is measured in
    `test_a_planes_frame_really_reads_that_way.py`, because a pane border belongs to no
    pane and `capture-pane` cannot see one.
    """

    def test_the_active_pane_is_marked(self):
        """`arrows` and not `off`. The frame charter ships has no surface, so
        `window-active-style` has no shade to be one step from and both rule styles are
        pinned to one value on purpose — which left nothing on screen saying which pane the
        keyboard was in."""
        self.assertEqual(commands_frame._chrome_values()["pane-border-indicators"],
                         "arrows")

    def test_the_two_rule_styles_are_still_one_value(self):
        """#514 is not reopened by it, and this is the assertion that says so. The cue is a
        GLYPH substituted into a rule that keeps one style along its whole length; a cue
        made of a second style would be a rule that changes appearance halfway, which is
        the defect that put charter in charge of these options."""
        values = commands_frame._chrome_values()
        self.assertEqual(values["pane-border-style"], values["pane-active-border-style"])

    def test_it_is_still_floored_where_the_option_arrived(self):
        """The option does not exist at `tmuxctl.FLOOR` — pinning a name tmux does not have
        is refused rather than degraded, which is #716, and the value moving does not move
        the floor."""
        floors = {name: floor for name, _v, floor in commands_frame._CHROME}
        # The NUMBER, not the constant compared to itself: `_CHROME` is written with the
        # constant in it, so naming it here would pass whatever either one said. 3.3 is
        # where tmux's own CHANGES file put the option, and it is the fact this row is
        # floored on.
        self.assertEqual(floors["pane-border-indicators"], (3, 3))
        self.assertEqual(commands_frame.tmuxctl.BORDER_INDICATORS_FLOOR, (3, 3))
        self.assertGreater(floors["pane-border-indicators"],
                           commands_frame.tmuxctl.FLOOR,
                           "an option floored at charter's own floor needs no floor")

    def test_it_is_still_pinned_rather_than_inherited(self):
        """Whatever the value, the point of the row is that the operator's own `.tmux.conf`
        does not decide it."""
        self.assertIn("pane-border-indicators", commands_frame._chrome_values())


class TheAccentRolesAreOneList(unittest.TestCase):
    """`[frame] ok`/`warn`/`bad` — the gap `[frame] text` named and could not close.

    `text` fixes a pane whose background is inverted relative to the terminal's. On a
    terminal that is already light it has nothing to do, and what is hard to read there is
    these three: **yellow on tan** is the pair no palette was designed for, and no amount of
    it being the operator's own yellow makes it legible on the operator's own tan.

    `instance.FRAME_ACCENTS` is the one list, and these are the joins that have to hold
    across it — a role configurable and not served, or served and not configurable, is a
    vocabulary charter enforces and nobody can use.
    """

    def test_every_accent_role_is_a_frame_key(self):
        for role in instance.FRAME_ACCENTS:
            with self.subTest(role=role):
                self.assertIn(role, instance.FRAME_FIELDS)
                self.assertIn(role, instance.FRAME_DEFAULTS)

    def test_every_accent_role_is_a_field_on_the_look(self):
        for role in instance.FRAME_ACCENTS:
            with self.subTest(role=role):
                self.assertIn(role, instance.Look._fields)

    def test_every_accent_role_reaches_a_provider_as_the_plane_s_own_colour(self):
        """`chrome.recipes` is what a component composes with, and a role charter lets a
        plane recolour without serving it would be two frames in one pane.

        Asked of the VALUE and not of the key: the keys are built by a comprehension over
        `FRAME_ACCENTS`, so "is this role served" cannot fail by construction. What can
        fail is a role served the colour charter always drew on a plane that asked for a
        different one — the wiring being present and inert."""
        with mock.patch.dict(config.FRAME, {"ok": "blue", "warn": "cyan", "bad": "magenta"}), \
                mock.patch.object(chrome, "colour_ok", lambda: True):
            got = chrome.recipes()
        for role, want in (("ok", "\x1b[34m"), ("warn", "\x1b[36m"),
                           ("bad", "\x1b[35m")):
            with self.subTest(role=role):
                self.assertEqual(got[role], want)

    def test_the_toml_key_is_the_role_name(self):
        """One word, so `docs/frame.md`'s hyphen rule does not arise — and the key an
        operator types is the word this table is indexed by."""
        for role in instance.FRAME_ACCENTS:
            with self.subTest(role=role):
                self.assertEqual(instance.FRAME_FIELDS[role][1], role)


class TheShippedAccentsAreTheOnesCharterAlwaysDrew(unittest.TestCase):
    """A plane that says nothing gets a byte-identical frame.

    **Against `FRAME_DEFAULTS` and never against the ambient plane**, which is this
    suite's own recurring defect said about the newest key: `statusline.accent` reads
    `config.FRAME`, and `config.FRAME` on a developer's machine is the plane the suite is
    running inside. Every assertion here is about the plane that said NOTHING, so the
    plane has to actually say nothing — otherwise the first operator to write
    `warn = "blue"` in the `charter.toml` this feature was written for turns three of
    these red, and the failure is about their config rather than about charter.

    The mechanism is new; the colours are not. This is the pair of sources agreeing —
    `statusline._SHIPPED_ACCENT` is what this module has drawn since before it had a key,
    and `_ACCENT_SGR` indexed by `FRAME_DEFAULTS` is what the new table derives.
    """

    def setUp(self):
        super().setUp()
        patch = mock.patch.dict(config.FRAME, instance.FRAME_DEFAULTS, clear=True)
        patch.start()
        self.addCleanup(patch.stop)

    def test_the_two_spellings_of_the_shipped_colour_agree(self):
        for role in instance.FRAME_ACCENTS:
            with self.subTest(role=role):
                self.assertEqual(sl._SHIPPED_ACCENT[role],
                                 sl._ACCENT_SGR[instance.FRAME_DEFAULTS[role]])

    def test_a_quiet_plane_draws_green_yellow_and_red(self):
        self.assertEqual(
            [sl.accent(r) for r in instance.FRAME_ACCENTS],
            [sl._GREEN, sl._YELLOW, sl._RED])

    def test_the_recipes_a_quiet_plane_serves_are_the_ones_it_always_served(self):
        got = chrome._role_values()
        self.assertEqual([got[r] for r in instance.FRAME_ACCENTS],
                         [sl._GREEN, sl._YELLOW, sl._RED])

    def test_a_frame_written_before_these_keys_existed_still_has_them(self):
        """**`look_of`'s `.get` with a default, and the case it is there for.** A frame
        relaunched by a charter that predates these keys has a `frame.state` with `rules`,
        `text` and `dim` in it and no accents at all — a subscript would raise inside the
        launcher, which is a plane with no frame rather than a plane with a green `✓`.

        Asked of a dict that really is missing them, because `config.FRAME` has been
        through `frame_of` and always carries all three: a test that went through the
        config boundary would exercise the key and never the fallback."""
        got = instance.look_of({"rules": "visible", "text": "black", "dim": False})
        for role in instance.FRAME_ACCENTS:
            with self.subTest(role=role):
                self.assertEqual(getattr(got, role), instance.FRAME_DEFAULTS[role])

    def test_each_accent_falls_back_on_its_own_and_not_on_a_neighbours(self):
        """The other half of the same line: a comprehension keyed on the wrong role would
        give all three the same colour and the assertion above would not see it, because
        the three shipped defaults are read from the same table it reads."""
        got = instance.look_of({"ok": "blue", "bad": "cyan"})
        self.assertEqual((got.ok, got.warn, got.bad), ("blue", "yellow", "cyan"))

    def test_the_shipped_vocabulary_is_exactly_what_it_was(self):
        """`chrome.served_params` is the containment #707 opened, and a key that widened it
        for a plane that said nothing would be a hole rather than a feature."""
        self.assertEqual(sorted(chrome.served_params()), [0, 1, 2, 7, 31, 32, 33])


class ThePlanesOwnPaletteReachesTheAccent(unittest.TestCase):
    """The keys doing something — asked of every word, because #687's warning bites in the
    direction of acceptance and a vocabulary that is documented and half-usable is worse
    than a smaller one."""

    def _with(self, **frame):
        return mock.patch.dict(config.FRAME, frame)

    def test_every_word_in_the_vocabulary_is_accepted(self):
        for word in instance.FRAME_PANE_FG:
            with self.subTest(word=word), self._with(warn=word):
                self.assertEqual(sl.accent("warn"), sl._ACCENT_SGR[word])

    def test_default_is_the_panes_own_foreground_and_not_a_hole(self):
        """SGR 39 — which is `[frame] text` where the plane set one and the terminal's own
        where it did not. So `warn = "default"` is "stop colouring the warnings", and the
        glyph still says what the row means."""
        with self._with(warn="default"):
            self.assertEqual(sl.accent("warn"), "\x1b[39m")

    def test_the_three_roles_are_independent(self):
        with self._with(ok="blue", warn="magenta", bad="cyan"):
            self.assertEqual([sl.accent(r) for r in instance.FRAME_ACCENTS],
                             ["\x1b[34m", "\x1b[35m", "\x1b[36m"])

    def test_a_word_charter_does_not_know_draws_what_it_always_drew(self):
        """`[frame]`'s degrade-rather-than-refuse contract (#535's right half): a typo costs
        the shipped colour in a frame that is otherwise entirely intact."""
        for hostile in ("chartreuse", "colour236", "", None, 7, ["blue"], {"a": 1}):
            with self.subTest(hostile=hostile):
                with self._with(warn=hostile):
                    self.assertEqual(sl.accent("warn"), sl._YELLOW)

    def test_the_word_is_refused_at_the_config_boundary(self):
        """Where `text` is refused, and by the same function — a second shape for "is this
        one of the seventeen" is how the two come to disagree about a word."""
        for hostile in ("chartreuse", "colour236", 7, ["blue"]):
            with self.subTest(hostile=hostile):
                self.assertEqual(instance.frame_of({"frame": {"warn": hostile}})["warn"],
                                 "yellow")
        self.assertEqual(instance.frame_of({"frame": {"warn": "blue"}})["warn"], "blue")

    def test_no_accent_can_name_a_cube_index_or_a_triple(self):
        """§3.2 said about the SGR side — `TheFrameNamesColoursAndNeverIndexes` renders
        every slot and asserts this, and the new table is inside that rule rather than an
        exception to it."""
        for word, sgr in sl._ACCENT_SGR.items():
            with self.subTest(word=word):
                self.assertRegex(sgr, r"^\x1b\[(3[0-9]|9[0-7])m$", f"{word} = {sgr!r}")


class ThePlanesAccentReachesARealRow(PersonaIso, unittest.TestCase):
    """The keys reaching a pane, which is the only place any of this counts.

    A plane setting a word and a `recipes()` dict agreeing about it proves the wiring and
    not the frame: charter's own renderers wrote those escapes directly, and routing them
    is the half #768 named as remaining work.
    """

    WIDE = 200

    def _branch(self, **frame):
        data = {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                "repos": [{"name": "alpha", "branch": "main", "dirty": True,
                           "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": "success",
                           "change": None, "sigil": "", "current": False,
                           "worktree_count": 0}],
                "worktrees": []}
        with mock.patch.dict(config.FRAME, frame):
            return slots._table_lines(data, self.WIDE, 6)[0].text

    def test_a_dirty_branch_is_yellow_on_a_plane_that_said_nothing(self):
        """The control. `warn` is a dirty branch here, and yellow on tan is the pair the
        key exists for — so the fixture has to actually draw one."""
        self.assertIn(33, _params(self._branch()))

    def test_a_plane_that_asked_for_blue_gets_blue(self):
        got = self._branch(warn="blue")
        self.assertIn(34, _params(got), repr(got))
        self.assertNotIn(33, _params(got), repr(got))

    def test_a_plane_that_asked_for_no_colour_at_all_still_says_it_is_dirty(self):
        """`NoStatusIsCarriedByColourAlone` is what makes `default` a usable answer rather
        than a loss: the row says the same thing with its escapes stripped."""
        self.assertEqual(chrome.plain(self._branch(warn="default")),
                         chrome.plain(self._branch()))

    def test_the_ci_table_resolves_its_role_rather_than_an_escape_from_import_time(self):
        """`statusline._CI_MARK` is the one accent site that is a module-level TABLE, so it
        is the one that would have frozen its colours before any plane was read."""
        with mock.patch.dict(config.FRAME, {"ok": "magenta"}):
            self.assertIn("\x1b[35m", sl._ci_part("success"))
        self.assertIn(sl._GREEN, sl._ci_part("success"))

    def test_every_pipeline_status_gitlab_reports_has_its_own_row(self):
        """**The KEYS of `_CI_MARK`, pinned, and they are pinned here because this change
        rewrote every value in that table.** They are GitLab's own status names off the
        wire, so a typo in one is not a colour that comes out wrong — it is a pipeline that
        falls through to the `?` fallback and reads as a status charter does not recognise.
        Nothing asserted them before, which the deletion sweep found by retuning
        `"pending"`, `"created"` and `"preparing"` and watching all 9 647 tests pass.

        The glyph and the label are pinned with the name, because what an operator reads is
        those two and the three names above are three different ways of saying `○ pending`.
        """
        for status, glyph, label in (
                ("success", "✓", "passed"), ("failed", "✗", "failed"),
                ("running", "●", "running"), ("pending", "○", "pending"),
                ("created", "○", "pending"), ("preparing", "○", "pending"),
                ("waiting_for_resource", "○", "queued"),
                ("scheduled", "○", "scheduled"), ("manual", "‖", "manual"),
                ("canceled", "⊘", "canceled"), ("skipped", "»", "skipped")):
            with self.subTest(status=status):
                self.assertEqual(chrome.plain(sl._ci_part(status)), f"{glyph} {label}")

    def test_a_status_charter_does_not_know_says_so_rather_than_guessing(self):
        """The control for the row above, and the behaviour a mistyped key degrades to: the
        name comes back verbatim behind a `?`, which is the honest answer to a status
        GitLab grew after this table was written."""
        self.assertEqual(chrome.plain(sl._ci_part("blocked")), "? blocked")
        self.assertEqual(sl._ci_part(""), "")
        self.assertEqual(sl._ci_part(None), "")

    def test_a_role_that_is_not_an_accent_keeps_its_own_colour(self):
        """`running` is a state rather than a verdict and `manual`/`skipped` are muted, so
        none of them moves with `ok`. A pass that recoloured the whole table would be
        charter deciding that every mark in it means the same three things."""
        with mock.patch.dict(config.FRAME, {"ok": "magenta", "warn": "blue",
                                            "bad": "cyan"}):
            self.assertIn(sl._CYAN, sl._ci_part("running"))
            self.assertIn(sl._DIM, sl._ci_part("skipped"))

    def test_a_repos_identity_colour_does_not_move_with_an_accent(self):
        """`statusline._PALETTE` cycles green and yellow as REPO identity, which is why the
        routing is at the call sites and not a substitution over the finished row: a pass
        over the pane would have recoloured a repo's name as though it were a status."""
        self.assertIn("\x1b[33m", sl._PALETTE)
        with mock.patch.dict(config.FRAME, {"warn": "blue"}):
            self.assertIn("\x1b[33m", sl._PALETTE)


class NoColourStillWinsOverThePlanesPalette(unittest.TestCase):
    """`NO_COLOR` is a superset of every key here and meets them in one ordering rather
    than fighting — `chrome.recipes`' own argument, re-asked now that three of the roles
    can be words."""

    def test_every_accent_role_is_empty_under_no_color(self):
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}), \
                mock.patch.dict(config.FRAME, {"warn": "blue"}):
            got = chrome.recipes()
            for role in instance.FRAME_ACCENTS:
                with self.subTest(role=role):
                    self.assertEqual(got[role], "")

    def test_the_vocabulary_the_plane_widened_is_still_admitted(self):
        """`served_params` is deliberately NOT emptied by `NO_COLOR` — `contain_row` asks
        `colour_ok` for itself one call earlier, and a vocabulary that shrank would be a
        second answer to a question already answered."""
        with mock.patch.dict(config.FRAME, {"warn": "blue"}):
            self.assertIn(34, chrome.served_params())
            self.assertIn(33, chrome.served_params(),
                          "a plane cannot narrow what a provider may write")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
