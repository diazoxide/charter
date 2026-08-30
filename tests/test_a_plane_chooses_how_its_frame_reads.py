"""`[frame] rules`, `[frame] text` and `[frame] dim` — the three words a plane says about
how charter's own chrome reads, and the boundary each of them stops at.

**The report, and the measurement under it.** An operator whose plane paints every panel
`bg = "brightblack"` reported two things within ten minutes of each other, on a terminal
whose theme renders that word as a light **tan**:

* a one-cell seam between every pair of panels — a dim default-foreground glyph sitting in
  the middle of a surface. tmux ALWAYS paints a border cell (`pane-border-lines` has no
  `none`: its five values are `single`, `double`, `heavy`, `simple` and `number`), so this
  was never a character question. It is a contrast question, and giving the glyph the
  surface's own colour is the whole answer. Reported four times now — #627, #631, #657, and
  again against the frame that fixed #657.
* the text on that surface. Captured off their live frame with `capture-pane -e`, charter
  paints `ESC[32m`, `ESC[34m`, `ESC[35m`, `ESC[2m`, `ESC[1m` and `ESC[7m` through eight
  recipes — and every one of those was chosen against a **dark** terminal. `bg` has been
  configurable across seventeen words since #631; the foreground drawn on it was not
  configurable at all.

**Charter cannot compute its way out of the second one, and that is established rather than
assumed.** The sixteen ANSI names have no fixed RGB — `brightblack` is a dark grey on most
themes and tan on this operator's — so "is this readable" is a question about a terminal
charter cannot see. This plane's own `charter.toml` already records why it cannot see it:
OSC 11 through tmux answers nothing, and `$COLORTERM` inside a pane describes the terminal
that started the SERVER rather than the one looking at the pane. A guard that claimed to
measure contrast would be exactly the overclaim this project refuses. So the plane is
asked, in the vocabulary it already answers `bg` in.

**Three keys and not eight, and the count is argued.** Of `chrome.recipes()`' eight roles,
`heading` (bold), `selected` (reverse), `reset` and `inset` are theme-safe by construction
and `frame/chrome.py` already argues why; `ok`/`warn`/`bad` are slots in the operator's own
palette, which is `instance.FRAME_PANE_COLOURS`' own argument for refusing `colour24`.
Renaming those slots per recipe would let a plane swap its own green for its own blue,
which is a choice and not a legibility fix. What is left is the two things that are NOT
slots: the default foreground, which the terminal chose to sit on its own background rather
than on the one charter painted, and `dim`, which is an attribute that always moves toward
the background. Those are `text` and `dim`. `rules` is the third because a rule is chrome
rather than text and has its own question — whether it is there at all.

The live half of all three — what tmux really does with the values these produce, on 3.7c
and at `tmuxctl.FLOOR` — is `tests/test_a_planes_frame_really_reads_that_way.py`.
"""

from __future__ import annotations

import io
import os
import re
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import chrome, pane, panel
from tests._isolation import PersonaIso

#: The frame a plane that has written nothing gets.
_SHIPPED = instance.SHIPPED_LOOK

#: A `window-style`/`pane-border-style` value is `bg=` and one word, always. Pinned below,
#: because `instance.rule_style` takes the colour out of one with `removeprefix`.
_BARE_BG = re.compile(r"^bg=[a-z]+$")


def _frame(**keys) -> dict:
    """A `[frame]` mapping `instance.frame_of` really produced, from the TOML spellings.

    Resolved rather than assembled, so what these tests read is what a committed
    `charter.toml` would have produced — refusals included.
    """
    return instance.frame_of({"frame": keys})


class TheThreeWordsAreCheckedWhereTheyEnter(unittest.TestCase):
    """`instance.frame_of` — the boundary, and what a value charter cannot read costs.

    **These are `[frame]` keys, so a word charter does not know degrades to the shipped
    default and does not refuse the frame.** That is deliberate and it is the OTHER side of
    #535's rule rather than a departure from it. #535 refuses an ARRANGEMENT whole because
    a component silently missing reads as a plane with no clones — a frame that is missing
    a pane looks like a machine that is missing repos, and no operator would suspect a
    typo. A `rules` word charter cannot read costs the operator a rule drawn the shipped
    way in a frame that is otherwise entirely intact, which is what every other key in this
    section already does (`chrome`, `density`, `hotkey`) and what `frame_of`'s own contract
    promises: a charter.toml charter cannot make sense of never stops charter from running.

    #687 is the standing warning against the other direction — a refusal is wrong when the
    value was in fact usable — and it applies here twice: `text = "brightblack"` and
    `rules = "hidden"` are usable words, so what has to be got right is that they are
    ACCEPTED, which is what the first tests below are about.
    """

    def test_the_shipped_frame_hides_its_rules_keeps_its_foreground_and_may_dim(self):
        self.assertEqual(_SHIPPED, instance.Look(rules="hidden", text="default", dim=True))
        self.assertEqual(instance.look_of(_frame()), _SHIPPED)

    def test_every_word_the_vocabularies_name_is_accepted(self):
        """#687's direction: the cost of refusing a word an operator meant is theirs, so
        each vocabulary is asserted whole rather than by sampling one member."""
        for word in instance.FRAME_RULES:
            with self.subTest(rules=word):
                self.assertEqual(instance.look_of(_frame(rules=word)).rules, word)
        for word in instance.FRAME_PANE_FG:
            with self.subTest(text=word):
                self.assertEqual(instance.look_of(_frame(text=word)).text, word)
        for flag in (True, False):
            with self.subTest(dim=flag):
                self.assertEqual(instance.look_of(_frame(dim=flag)).dim, flag)

    def test_the_two_vocabularies_are_the_ones_the_operator_already_knows(self):
        """`text` says the seventeen words `bg` says, and `rules` says two. A plane that
        learned one table has learned the other, which is the whole reason `text` is not
        spelled with numbers or hex."""
        self.assertEqual(set(instance.FRAME_PANE_FG), set(instance.FRAME_PANE_BG))
        self.assertEqual(instance.FRAME_RULES, ("hidden", "visible"))

    def test_a_word_charter_does_not_know_leaves_the_frame_whole_and_shipped(self):
        """Degrades, and only for the key that said it: the arrangement is not refused and
        the two keys beside it are untouched."""
        got = _frame(rules="invisible", text="tan", dim=True)
        self.assertEqual(instance.look_of(got), _SHIPPED)
        self.assertEqual(got["slots"], instance.FRAME_DEFAULTS["slots"],
                         "an unreadable appearance word took the frame's shape with it")

    def test_a_style_string_never_becomes_a_rule_or_a_foreground(self):
        """The sharp reason these are closed enums rather than `isinstance(value, str)`.
        A tmux style value is FORMAT-EXPANDED at draw time — measured on this very option,
        `set -w pane-border-style 'bg=#{?#{==:1,1},colour196,colour46}'` is rc 0 and reaches
        the wire as `ESC[48;5;196m` — and `charter.toml` arrives from someone else's
        machine."""
        hostile = "bg=#{?#{==:1,1},colour196,colour46}"
        look = instance.look_of(_frame(rules=hostile, text=hostile))
        self.assertEqual(look, _SHIPPED)
        for surface in (None, "bg=brightblack"):
            value = instance.rule_style(surface, commands_frame._CHROME_FG, look)
            self.assertNotIn("#", value)
        for _name, value in instance.surface_options("brightblack", "dark", look):
            self.assertNotIn("#", value)

    def test_a_value_of_the_wrong_type_is_refused_without_raising(self):
        """`instance` is imported by every command including `charter --version`, so a
        hand-edited file has to degrade rather than raise — and `tomllib` can hand this
        key a list or a table, which is why the membership test is guarded by
        `isinstance`."""
        for value in (7, ["hidden"], {"a": 1}, True, None):
            with self.subTest(value=value):
                self.assertEqual(instance.look_of(_frame(rules=value, text=value)),
                                 _SHIPPED)

    def test_dim_refuses_an_int_the_way_every_bool_key_in_this_section_does(self):
        """`isinstance(True, int)` is True in Python, so the type check is written both
        ways round — `dim = 1` is not a `false` spelled oddly, it is a value charter
        cannot read."""
        for value in (1, 0, "false", "no"):
            with self.subTest(value=value):
                self.assertIs(instance.look_of(_frame(dim=value)).dim, True)

    def test_the_toml_spellings_are_the_words_docs_frame_md_documents(self):
        """One word each, so `docs/frame.md`'s hyphen rule (`history-limit`, never
        `history_limit`) does not arise — and the underscore form is not a second,
        undocumented alias, which is `FRAME_FIELDS`' own contract."""
        for key in ("rules", "text", "dim"):
            with self.subTest(key=key):
                self.assertEqual(instance.FRAME_FIELDS[key][1], key)

    def test_a_frame_relaunched_by_an_older_charter_still_answers(self):
        """`look_of` reads with `.get` and the shipped default, for `component_style`'s own
        reason: a frame whose record predates these keys is a real dict on a real disk, and
        a panel repaint must not raise a `KeyError` into a pane."""
        self.assertEqual(instance.look_of({}), _SHIPPED)
        self.assertEqual(instance.look_of({"slots": ["top"], "chrome": "dark"}), _SHIPPED)


class TheShippedRuleIsStillTheOneCharterAlwaysDrew(unittest.TestCase):
    """The default's safety, asserted rather than argued: `hidden` changes nothing at all
    for a plane that has no surface to hide a rule into.

    That is what makes it a defensible default where `chrome`'s is `off`. The two are not
    symmetric — `chrome = "dark"` would repaint a stranger's terminal, and a stranger's
    terminal is the one thing charter cannot see — while `hidden` has an effect ONLY where
    the plane has already written a `chrome` or a `bg` by hand, which is a plane that has
    already said it wants panels rather than boxes.
    """

    def test_the_table_and_the_assembler_still_agree_about_the_shipped_style(self):
        """`_CHROME_STYLE` is both the value a surfaceless frame carries and the MARKER
        `_chrome_argvs` uses to find which of the five options are styles, so the day the
        shipped `Look` changes this fails loudly instead of the marker quietly matching
        nothing."""
        self.assertEqual(
            instance.rule_style(None, commands_frame._CHROME_FG, _SHIPPED),
            commands_frame._CHROME_STYLE)

    def test_a_surfaceless_frame_is_byte_identical_under_either_word(self):
        for word in instance.FRAME_RULES:
            with self.subTest(rules=word):
                self.assertEqual(
                    instance.rule_style(None, commands_frame._CHROME_FG,
                                        instance.Look(word, "default", True)),
                    commands_frame._CHROME_STYLE)

    def test_a_text_colour_with_no_surface_still_reaches_the_rule(self):
        """The arm no other case gets to: `chrome = "off"`, no `bg` anywhere, and a `text`
        word. There is no background to hide a rule in, so `hidden` cannot apply and the
        *visible* assembly runs — but the plane HAS said what colour its frame's text is,
        and the rule is chrome drawn on the same panes. So it takes the foreground and the
        `dim`, and no background clause at all.

        Byte-identical to `_CHROME_STYLE` only when the plane named no foreground either,
        which is what the case above pins. The two together are the whole of what a
        surfaceless frame can carry."""
        self.assertEqual(
            instance.rule_style(None, commands_frame._CHROME_FG,
                                instance.Look("hidden", "black", True)),
            "fg=black,dim")
        self.assertEqual(
            instance.rule_style(None, commands_frame._CHROME_FG,
                                instance.Look("hidden", "black", False)),
            "fg=black")

    def test_the_five_pinned_options_are_untouched_by_any_of_the_three_words(self):
        """`hidden` decides a colour, `text` decides a colour and `dim` decides an
        attribute. None of them is a line weight, an indicator or a border status — and
        `pane-border-lines` has no `none` for one of them to reach for even if it wanted
        to."""
        # Read through `_chrome_values`, which is #716's own unpacking of a row that is
        # three fields wide now — the floor decides which tmuxes are TOLD about an option
        # and is a different question from what the option carries, so a version high
        # enough to be told about all five is what these are asked at.
        not_styles = [(n, v) for n, v in commands_frame._chrome_values().items()
                      if v != commands_frame._CHROME_STYLE]
        self.assertTrue(not_styles)
        every_option = max(f for _n, _v, f in commands_frame._CHROME)
        for look in (_SHIPPED, instance.Look("visible", "red", False)):
            got = {a[-2]: a[-1] for a in commands_frame._chrome_argvs(
                socket="s", harness_pane="%1", surface="bg=blue", look=look,
                v=every_option)}
            for name, value in not_styles:
                with self.subTest(look=look, option=name):
                    self.assertEqual(got[name], value)


class TheSurfaceIsAlwaysABareBackgroundClause(unittest.TestCase):
    """The property `instance.rule_style`'s `removeprefix("bg=")` rests on.

    A hidden rule's foreground is the only value in the frame that is DERIVED from another
    charter constant rather than looked up in a table, so the shape it derives from is
    pinned here over both tables at once. A `window-style` entry that grew an `fg` or an
    attribute would otherwise make `hidden` emit `fg=fg=black,bg=black`, which tmux refuses
    — reported as one panel's rules failing to paint, in a call `tmuxctl.run` treats as
    non-fatal.
    """

    def test_every_surface_charter_can_name_is_bg_and_one_word(self):
        seen = 0
        for table in (instance.FRAME_PANE_BG, instance.FRAME_CHROME):
            for word, pairs in table.items():
                for name, value in pairs:
                    with self.subTest(word=word, option=name):
                        self.assertRegex(value, _BARE_BG)
                        seen += 1
        self.assertGreater(seen, 30, "almost nothing was checked")

    def test_the_foreground_table_is_the_mirror_of_it(self):
        """`fg=` and one word, or nothing at all for `default` — which is not a hole: a
        pane whose style names no `fg` already draws in the terminal's own foreground, so
        the empty clause is what keeps a plane that says nothing byte-identical."""
        self.assertEqual(instance.FRAME_PANE_FG["default"], "")
        self.assertEqual(instance.text_fg("default"), "")
        for word, clause in instance.FRAME_PANE_FG.items():
            if word != "default":
                with self.subTest(word=word):
                    self.assertEqual(clause, f"fg={word}")


class AHiddenRuleTakesTheSurfaceAndNeverAnAttribute(unittest.TestCase):
    """`instance.rule_style` — the three decisions, one input each.

    The value the operator hand-applied to their own live frame and confirmed is
    `fg=brightblack,bg=brightblack`: the surface on both halves, and **no `dim`**. Every
    arm below is reached by an input that reaches only it.
    """

    FG = commands_frame._CHROME_FG

    def test_a_hidden_rule_over_a_colour_is_that_colour_twice(self):
        for word in instance.FRAME_PANE_BG:
            surface = dict(instance.FRAME_PANE_BG[word])["window-style"]
            if surface == "bg=default":
                continue
            with self.subTest(bg=word):
                self.assertEqual(
                    instance.rule_style(surface, self.FG, _SHIPPED),
                    f"fg={surface.removeprefix('bg=')},{surface}")

    def test_hidden_never_appends_dim_even_where_the_plane_left_it_on(self):
        """The attribute is dropped by the ARM and not by the `dim` key: an invisible glyph
        made visibly something is not what the word asked for, and the style the operator
        confirmed carries no attribute."""
        self.assertNotIn(
            "dim", instance.rule_style("bg=blue", self.FG,
                                       instance.Look("hidden", "default", True)))

    def test_a_hidden_rule_over_the_terminals_own_background_stays_visible(self):
        """**`bg=default` is a real committed arrangement**, not a defensive case: it is
        the seventeenth `FRAME_PANE_BG` word and a component names it to opt one pane out
        of a frame-wide `chrome`. It means "the terminal's own background", and there is no
        `fg=` spelling for that — `fg=default` is the terminal's own FOREGROUND, the one
        colour guaranteed to be visible against it. Honouring `hidden` there would draw the
        rule at FULL strength, brighter than the `dim` it replaced, which is the opposite of
        the word. So it falls through, and the input that reaches this arm reaches no
        other."""
        self.assertEqual(instance.rule_style("bg=default", self.FG, _SHIPPED),
                         f"{self.FG},dim,bg=default")

    def test_a_visible_rule_is_the_frames_foreground_dimmed_over_the_surface(self):
        self.assertEqual(
            instance.rule_style("bg=brightblack", self.FG,
                                instance.Look("visible", "default", True)),
            f"{self.FG},dim,bg=brightblack")

    def test_a_visible_rule_takes_the_planes_own_text_colour_where_it_named_one(self):
        """A frame whose panes carry a foreground draws its rules in it rather than in a
        colour nothing else on the screen is wearing — `border_bg`'s argument about the
        background, said one clause over. The input reaches only this arm: `text` changes
        nothing under `hidden`, where the foreground is the surface's."""
        self.assertEqual(
            instance.rule_style("bg=brightblack", self.FG,
                                instance.Look("visible", "black", True)),
            "fg=black,dim,bg=brightblack")
        self.assertEqual(
            instance.rule_style("bg=brightblack", self.FG,
                                instance.Look("hidden", "black", True)),
            "fg=brightblack,bg=brightblack",
            "a `text` colour leaked into a rule that is supposed to be invisible")

    def test_dim_off_drops_the_attribute_and_touches_nothing_else(self):
        """The one input that reaches this arm and no other, on both sides of the surface
        question — and the reason `[frame] dim` is a frame-wide word rather than a text
        one: `fg=default` at full strength is the operator's own foreground, so it is the
        only way to make the frame's own separation louder that cannot be wrong on a theme
        charter cannot see."""
        loud = instance.Look("visible", "default", False)
        self.assertEqual(instance.rule_style(None, self.FG, loud), self.FG)
        self.assertEqual(instance.rule_style("bg=blue", self.FG, loud),
                         f"{self.FG},bg=blue")

    def test_a_word_that_is_not_hidden_draws_the_visible_rule(self):
        """`rule_style` compares against a constant rather than validating, because the
        word selects a BRANCH and is not a value that reaches tmux. `frame_of` is the
        boundary; a word that got past nothing still cannot be `"hidden"`."""
        for word in ("Hidden", "hide", "", None, 7):
            with self.subTest(rules=word):
                self.assertEqual(
                    instance.rule_style("bg=blue", self.FG,
                                        instance.Look(word, "default", True)),
                    f"{self.FG},dim,bg=blue")


class TheForegroundIsPaintedByTmuxAndNotByARenderer(unittest.TestCase):
    """`instance.surface_options` — one key, every cell, and no renderer told anything.

    `window-style` carries an `fg` exactly as it carries a `bg`, and tmux resolves a pane's
    DEFAULT foreground from it — so charter's own `ESC[0m` returns to the plane's colour
    rather than the terminal's, and the forty `statusline._DIM` call sites in
    `frame/slots.py` and a provider's component alike are covered without being asked. The
    rendering that makes that true is measured in
    `tests/test_a_planes_frame_really_reads_that_way.py`.
    """

    def test_a_plane_that_named_no_foreground_emits_exactly_what_it_did_before(self):
        for bg in (None, "brightblack", "nonsense"):
            for level in instance.FRAME_CHROME:
                with self.subTest(bg=bg, chrome=level):
                    self.assertEqual(
                        instance.surface_options(bg, level, _SHIPPED),
                        instance.pane_bg_options(bg) or instance.chrome_options(level))

    def test_the_foreground_joins_the_background_on_both_options(self):
        """Both, always — a pane whose two differed would show one colour focused and
        another unfocused, which is `pane_bg_options`' own rule about the background."""
        got = dict(instance.surface_options("brightblack", "off",
                                            instance.Look("hidden", "black", True)))
        self.assertEqual(got, {"window-style": "fg=black,bg=brightblack",
                               "window-active-style": "fg=black,bg=black"})

    def test_a_foreground_with_no_surface_at_all_is_still_written(self):
        """`chrome = "off"`, no `bg` anywhere, `text = "black"`: a real arrangement. There
        is no background to paint and the plane has still said what colour its frame's text
        is, so the two style options carry the foreground alone."""
        self.assertEqual(
            dict(instance.surface_options(None, "off",
                                          instance.Look("hidden", "black", True))),
            {name: "fg=black" for name in instance.chrome_option_names()})

    def test_a_plane_that_said_neither_writes_nothing(self):
        self.assertEqual(instance.surface_options(None, "off", _SHIPPED), ())

    def test_no_operator_string_reaches_a_pane_style(self):
        """The containment on both halves: the background comes out of `FRAME_PANE_BG` or
        `FRAME_CHROME` as it always did, and the foreground out of `FRAME_PANE_FG`, each
        indexed by a word that was only ever a key."""
        allowed = {v for pairs in instance.FRAME_PANE_BG.values() for _n, v in pairs}
        allowed |= {v for pairs in instance.FRAME_CHROME.values() for _n, v in pairs}
        allowed |= set(instance.FRAME_PANE_FG.values())
        for word in (*instance.FRAME_PANE_FG, "colour236", "chartreuse", None, 7,
                     "fg=#{?#{==:1,1},colour196,colour46}", ["blue"], True):
            with self.subTest(text=word):
                pairs = instance.surface_options("brightblack", "dark",
                                                 instance.Look("hidden", word, True))
                self.assertTrue(pairs)
                for _name, value in pairs:
                    self.assertNotIn("#", value)
                    for clause in value.split(","):
                        self.assertIn(clause, allowed)

    def test_the_rule_reads_the_background_and_not_the_whole_surface(self):
        """`pane_border_options` resolves its colour off `pane_bg_options(bg) or
        chrome_options(chrome)` and never off `surface_options`, and the difference is
        load-bearing rather than an oversight: a rule resolved off a pane style that
        already carries the plane's `text` would come out with two foregrounds in it."""
        look = instance.Look("visible", "red", True)
        for _name, value in instance.pane_border_options("brightblack", "dark",
                                                         commands_frame._CHROME_FG, look):
            with self.subTest(value=value):
                self.assertEqual(value.count("fg="), 1)
                self.assertEqual(value, "fg=red,dim,bg=brightblack")

    def test_no_colour_refuses_the_foreground_with_the_background(self):
        """A `text` word IS a colour charter was asked to paint, so it goes with the
        surface rather than surviving beside it — and on the live path that means the
        unsets rather than silence, because a frame surfaced before `NO_COLOR` was exported
        has values to remove."""
        look = instance.Look("hidden", "black", True)
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}, clear=True):
            self.assertEqual(
                commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="off",
                                              bg=None, look=look), [])
            tails = [a[a.index("set-option"):] for a in commands_frame._resurface_argvs(
                socket="s", pane_id="%3", chrome="off", bg=None, look=look)]
            for name in instance.chrome_option_names():
                self.assertIn(["set-option", "-p", "-u", "-t", "%3", name], tails)

    def test_the_live_path_sets_the_foreground_where_the_launch_path_does(self):
        """`_surface_argvs` and `_resurface_argvs` resolve the pane through the same
        expression, which is why `look` is a parameter of both rather than something either
        looks up: a pane painted one way at launch and another way on a palette keypress is
        #547's shape on a surface the operator can see."""
        look = instance.Look("hidden", "black", True)
        kw = dict(socket="s", pane_id="%3", chrome="dark", bg="brightblack", look=look)
        with mock.patch.dict(os.environ, {}, clear=True):
            launch = {a[-2]: a[-1] for a in commands_frame._surface_argvs(**kw)}
            live = {a[-2]: a[-1] for a in commands_frame._resurface_argvs(**kw)
                    if "-u" not in a}
        self.assertEqual(launch, live)
        self.assertEqual(launch["window-style"], "fg=black,bg=brightblack")


class DimIsDeletedFromTheFinishedRow(unittest.TestCase):
    """`chrome.undim` — the property is the NUMBER and the position, not the spelling.

    `chrome.resets_everything`'s lesson, applied to a different parameter. The extra hazard
    here is that **2 is also a colour SPACE**: `ESC[38;2;255;0;0m` is a 24-bit foreground,
    and a pass that filtered every 2 out of a parameter list would turn it into
    `ESC[38;255;0;0m` and hand a terminal a colour nobody asked for. Charter writes no such
    escape — `FRAME_PANE_COLOURS` argues at length why it names the sixteen — which is
    exactly what makes the case real: this runs on a row a PROVIDER's component finished,
    and `tui.sanitize` passes any `ESC[<digits and semicolons>m` through untouched.
    """

    def test_every_spelling_of_dim_goes_and_nothing_beside_it_does(self):
        for row, want in (("\x1b[2mDIM\x1b[0m", "DIM\x1b[0m"),
                          ("\x1b[02mx", "x"),
                          ("\x1b[1;2mx", "\x1b[1mx"),
                          ("\x1b[2;32mx", "\x1b[32mx"),
                          ("\x1b[22;2;35mx", "\x1b[22;35mx"),
                          ("\x1b[7m\x1b[2m\x1b[0m", "\x1b[7m\x1b[0m")):
            with self.subTest(row=row):
                self.assertEqual(chrome.undim(row), want)

    def test_a_colour_space_of_two_is_a_colour_and_not_an_attribute(self):
        for row in ("\x1b[38;2;255;0;0mRED\x1b[0m", "\x1b[48;2;0;0;0mBG",
                    "\x1b[58;2;1;2;3mUL", "\x1b[38;5;196mx", "\x1b[38;5;2mx"):
            with self.subTest(row=row):
                self.assertEqual(chrome.undim(row), row)

    def test_a_dim_after_a_true_colour_is_still_deleted(self):
        """The walk has to resume at the right place, or the sub-parameters swallow the
        attribute that follows them."""
        self.assertEqual(chrome.undim("\x1b[48;2;0;0;0;2mBG"), "\x1b[48;2;0;0;0mBG")

    def test_an_escape_that_emptied_is_removed_rather_than_reset(self):
        """`ESC[m` is an SGR with an omitted parameter, and an omitted parameter takes
        SGR's default of ZERO — so emitting it for a `ESC[2m` that had nothing else in it
        would replace "turn dim on" with "turn everything off", cancelling the colour of
        whatever span the row was in the middle of."""
        self.assertEqual(chrome.undim("\x1b[32ma\x1b[2mb\x1b[0m"),
                         "\x1b[32mab\x1b[0m")
        self.assertNotIn("\x1b[m", chrome.undim("\x1b[2mb"))

    def test_a_reset_inside_the_list_survives_the_deletion(self):
        """`ESC[2;m` really did carry a reset, and the reset is not dim's to take with
        it."""
        self.assertEqual(chrome.undim("\x1b[2;mx"), "\x1b[mx")

    def test_it_deletes_and_never_substitutes(self):
        """Honest to run over a row charter did not write: it removes an instruction to
        reduce contrast and leaves every colour the component chose exactly where it put
        it. A pass that remapped a provider's green would be charter deciding what somebody
        else's component means."""
        row = "\x1b[35m▸ \x1b[1mname\x1b[0m \x1b[2m47\x1b[0m \x1b[32mok\x1b[0m"
        got = chrome.undim(row)
        for span in ("\x1b[35m", "\x1b[1m", "\x1b[32m", "\x1b[0m"):
            self.assertIn(span, got)
        self.assertNotIn("\x1b[2m", got)

    def test_a_row_with_no_escape_at_all_comes_back_unchanged(self):
        for row in ("", "plain text", "  ⋯ gathering"):
            with self.subTest(row=row):
                self.assertIs(chrome.undim(row), row)

    def test_it_changes_no_cell_of_width(self):
        """Every SGR costs zero columns, so this only ever makes one shorter or removes
        it — which is what lets it run AFTER `chrome.fill` clamped the row to the pane."""
        from charter import tui
        for row in ("\x1b[2mDIM\x1b[0m two", "\x1b[1;2mboth\x1b[0m",
                    "\x1b[38;2;9;9;9mtrue\x1b[0m"):
            with self.subTest(row=row):
                self.assertEqual(tui.width(chrome.undim(row)), tui.width(row))


class TheDimKeyReachesEveryRowAndNoColourStillWins(PersonaIso, unittest.TestCase):
    """`chrome.dim_ok`, `chrome.recipes` and `panel._write` — one reading, three sites.

    **`NO_COLOR` keeps winning and nothing here adds a second reading of it.**
    `chrome.no_colour` is still the one place that variable is read; `dim_ok` is a
    different question about a different input, and the two meet in exactly one ordering:
    no colour empties every SGR role including this one, so a plane that turned dim off and
    an operator who turned colour off do not fight — the second is a superset of the first.
    """

    def _look(self, **keys) -> dict:
        return {**config.FRAME, **_frame(**keys)}

    def test_recipes_empties_the_muted_role_and_leaves_the_others_alone(self):
        with mock.patch.object(chrome, "colour_ok", return_value=True):
            with mock.patch.dict(config.FRAME, self._look(dim=False)):
                off = dict(chrome.recipes())
            with mock.patch.dict(config.FRAME, self._look(dim=True)):
                on = dict(chrome.recipes())
        self.assertEqual(on["muted"], "\033[2m")
        self.assertEqual(off["muted"], "")
        self.assertEqual({k: v for k, v in off.items() if k != "muted"},
                         {k: v for k, v in on.items() if k != "muted"})

    def test_it_is_asked_as_the_property_and_not_by_the_roles_name(self):
        """`undim` is run over the values, so a role added with a dim in it is covered on
        the day it is added rather than being the one that still reduces contrast on a
        plane that asked charter not to."""
        with mock.patch.object(chrome, "colour_ok", return_value=True), \
                mock.patch.object(chrome, "_role_values",
                                  return_value={"muted": "\033[2m",
                                                "footnote": "\033[2;35m"}), \
                mock.patch.dict(config.FRAME, self._look(dim=False)):
            got = dict(chrome.recipes())
        self.assertEqual(got["footnote"], "\033[35m")

    def test_no_colour_empties_every_role_whichever_way_dim_is_set(self):
        for flag in (True, False):
            with self.subTest(dim=flag):
                with mock.patch.dict(os.environ, {"NO_COLOR": ""}, clear=True), \
                        mock.patch.dict(config.FRAME, self._look(dim=flag)):
                    got = dict(chrome.recipes())
                self.assertEqual({k: v for k, v in got.items() if k != "inset"},
                                 {k: "" for k in chrome._role_values()})

    def test_the_inset_survives_both_because_two_spaces_are_not_colour(self):
        """A frame that lost its inset under either key would be answering a question about
        colour with a change to LAYOUT — `recipes`' own rule, restated for the second
        key."""
        for env in ({}, {"NO_COLOR": ""}):
            for flag in (True, False):
                with self.subTest(no_color=bool(env), dim=flag):
                    with mock.patch.dict(os.environ, env, clear=True), \
                            mock.patch.dict(config.FRAME, self._look(dim=flag)):
                        self.assertTrue(chrome.recipes()["inset"].strip() == "")
                        self.assertTrue(chrome.recipes()["inset"])

    def test_turning_dim_off_does_not_make_a_providers_dim_print_as_text(self):
        """**The seam between this key and #722, and it is the one that could bite.**

        `chrome.served_params` decides which escapes in a FOREIGN pane's row survive
        `contain_row` and which are escaped into visible text — #707 was a provider's
        colour arriving in its pane as `^[[32m`. It is derived from `_role_values`, which
        this key does not touch, so SGR 2 stays admitted whatever the plane said and a
        provider's dim is deleted on the way out rather than printed on the way in.

        Derived from `recipes()` instead — which is the obvious other wiring, and the one
        that reads as tidier — `dim = false` would have re-opened #707 for exactly one
        parameter: a component's own `ESC[2m` escaped into its pane as four characters of
        garbage, on the planes that had asked for less visual noise. The two questions are
        different and the layering is what keeps them apart: admission is about charter's
        VOCABULARY, and `[frame] dim` is about whether the frame spends it."""
        for flag in (True, False):
            with self.subTest(dim=flag):
                with mock.patch.dict(config.FRAME, self._look(dim=flag)):
                    self.assertIn(2, chrome.served_params())
                    self.assertTrue(chrome.is_recipe("2", chrome.served_params()))

    def _painted(self, text: str, *, frame: dict, env: dict) -> str:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            # The pane's own descriptor answers the colour question (`frame/pane.py`,
            # #606), never `sys.stdout` — which under `redirect_stdout` is a `StringIO`
            # with no terminal behind it. Patched at that seam rather than at
            # `chrome.colour_ok`, so the `NO_COLOR` branch below is the real one.
            with mock.patch.object(pane, "is_tty", return_value=True), \
                    mock.patch.object(sys.stdout, "fileno", return_value=1,
                                      create=True), \
                    mock.patch("os.get_terminal_size",
                               return_value=os.terminal_size((40, 6))), \
                    mock.patch.dict(os.environ, env, clear=True), \
                    mock.patch.dict(config.FRAME, frame):
                panel._write(text)
        return out.getvalue()

    def test_the_panel_strips_dim_from_every_row_it_paints(self):
        """The siting is the whole of why one key reaches the whole frame: `_write` is the
        one place anything reaches a pane's screen, so charter's own renderers and a
        provider's component are covered by the same answer and neither has to ask."""
        rows = "\x1b[2mmuted\x1b[0m\n\x1b[32mok\x1b[0m \x1b[1;2mboth\x1b[0m"
        got = self._painted(rows, frame=self._look(dim=False), env={})
        self.assertNotIn("\x1b[2m", got)
        self.assertNotIn("\x1b[1;2m", got)
        self.assertIn("\x1b[1m", got)
        self.assertIn("\x1b[32m", got)
        self.assertIn("muted", got)

    def test_a_plane_that_left_dim_on_is_painted_byte_for_byte_as_before(self):
        rows = "\x1b[2mmuted\x1b[0m"
        self.assertIn("\x1b[2mmuted", self._painted(rows, frame=self._look(dim=True),
                                                    env={}))

    def test_no_colour_still_takes_everything_including_the_dim(self):
        """The ordering, asserted: the strip is unreachable under `NO_COLOR` rather than
        skipped by it, and what an operator gets is the same pane either way."""
        rows = "\x1b[2mmuted\x1b[0m \x1b[32mok\x1b[0m"
        for flag in (True, False):
            with self.subTest(dim=flag):
                got = self._painted(rows, frame=self._look(dim=flag),
                                    env={"NO_COLOR": ""})
                self.assertNotIn("\x1b[", got.split("\x1b[2J", 1)[1])
                self.assertIn("muted ok", got)
