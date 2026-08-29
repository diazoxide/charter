"""The frame's RULES carry the frame's surface — `instance.border_bg` and the two
`_CHROME` style options it reaches.

**The defect, as it was reported.** With `chrome = "dark"` and `bg = "brightblack"` on all
four components, every panel came out grey and a one-cell DARK STRIP ran between each pair
of them and around the sidebar — the frame read as coloured rectangles separated by seams
rather than as an application. `window-style` paints a pane's INTERIOR; the cell between
two panes is in neither pane, and tmux draws it from `pane-border-style`, which charter has
pinned to `commands_frame._CHROME_STYLE` (an `fg` and an attribute, no `bg`) since #514.

Measured on 3.7c, three panes each `bg = "brightblack"`, read off an attached client's
wire through a nested tmux::

    before  '\\x1b[100m … \\x1b[2m\\x1b[49m│\\x1b[0m\\x1b[100m'   <- ESC[49m: the seam
    after   '\\x1b[100m … \\x1b[2m│\\x1b[0m\\x1b[100m'            <- the surface runs through

The rendered half of that lives in `tests/test_frame_tmux_integration.py`'s
`ChromeIsOneColour`, which already owns the nested-tmux screenshot harness. This file is
the argv and the table — what charter decides, before any tmux is asked.

**Window-scoped, and that is a measurement rather than a preference.** `pane-border-style`
IS a pane option on 3.7c, and tmux there draws each border cell from the pane ABOVE or LEFT
of it and ignores the other side's outright (window blue + left pane red + right pane green
renders red). At `tmuxctl.FLOOR` it is not a pane option at all: `set -p` is rc 0 but
writes the WINDOW's value, `show -p` on a pane nobody set answers the window's, and
`set -p -u` removes charter's own #514 pin for the whole window. So a per-side design is
unavailable at the floor, silently window-wide there, and one-sided where it exists — the
rule takes ONE colour, set `-w`, exactly as `_chrome_argvs` already sets the other four.
"""

from __future__ import annotations

import os
import subprocess
import unittest
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import state, tmuxctl
from tests._isolation import PersonaIso

_STYLES = ("pane-border-style", "pane-active-border-style")


def _frame(*bgs, **rest) -> dict:
    """A `[frame]` mapping whose arrangement names one component per *bgs* entry — `None`
    for a component that names no background of its own.

    Built by hand, and only for `instance.border_bg`, which reads a placement's `bg` and
    nothing else. Anything that goes near `frame/layout.py` needs :func:`_resolved`, whose
    placements carry the `edge` and `size` a split is computed from.
    """
    return {"components": [{"slot": f"s{i}", "use": f"s{i}",
                            **({} if bg is None else {"bg": bg})}
                           for i, bg in enumerate(bgs)], **rest}


def _resolved(*bgs, **rest) -> dict:
    """A frame `instance.frame_of` really produced, for the tests that drive a launch.

    Resolved rather than assembled: what these then patch into `config.FRAME` is what a
    real `charter.toml` would have produced, refusals included — the same trade
    `tests/test_frame_pane_style.py`'s own `_arrangement` makes.
    """
    tables = [{"use": cid, **({} if bg is None else {"bg": bg})}
              for cid, bg in zip(("identity", "attention", "repos", "sidebar"), bgs)]
    return {**instance.frame_of({"frame": {"component": tables}}), **rest}


class TheRuleTakesTheSurfaceTheComponentsAgreeOn(unittest.TestCase):
    """`instance.border_bg` — which colour, and why that one.

    A border sits BETWEEN two panes, so with per-component `bg` the two sides may be
    different colours and no single value matches both. The rule takes the surface the
    components agree on, and the frame-wide `[frame] chrome` surface when they do not: a
    gutter in the frame's own colour between two differently-coloured panels is what an
    application looks like, while a gutter in NO colour between two identically-coloured
    panels is the seam this exists to close.
    """

    def test_components_that_all_name_one_colour_give_the_rule_that_colour(self):
        """The reported case. Four panels, one word, and the rule is that word's own
        `window-style` value — not the frame-wide `chrome`'s, which is a colour nothing
        else on the screen is wearing."""
        self.assertEqual(
            instance.border_bg(_frame(*["brightblack"] * 4), "dark"), "bg=brightblack")

    def test_components_that_disagree_leave_the_rule_the_frame_wide_surface(self):
        """No colour matches both sides, so the rule stops trying to and becomes the
        frame's own gutter — which is the colour every pane that named nothing is already
        wearing."""
        self.assertEqual(instance.border_bg(_frame("blue", "brightblack"), "dark"),
                         dict(instance.FRAME_CHROME["dark"])["window-style"])

    def test_naming_no_colour_agrees_with_naming_the_colour_chrome_already_is(self):
        """"They agree" is resolved through the same expression a PANE is resolved
        through — `pane_bg_options(bg) or chrome_options(chrome)` — so it means what an
        operator can see rather than "they wrote the same word". A component with no `bg`
        takes the frame-wide surface, and a component that named that surface's own colour
        is the same colour as it."""
        self.assertEqual(instance.border_bg(_frame("black", None), "dark"), "bg=black")
        self.assertEqual(instance.border_bg(_frame("brightblack", None), "dark"),
                         dict(instance.FRAME_CHROME["dark"])["window-style"],
                         "a `brightblack` panel beside an unpainted one is two colours, "
                         "and the rule must not claim to match both")

    def test_a_plane_with_no_arrangement_takes_the_frame_wide_surface(self):
        """`[frame] slots` and no `[[frame.component]]`: there are no per-pane colours to
        agree about, every pane is the frame-wide word, and so is the rule."""
        for level in instance.FRAME_CHROME:
            with self.subTest(level=level):
                self.assertEqual(instance.border_bg({}, level),
                                 dict(instance.FRAME_CHROME[level]).get("window-style"))

    def test_off_with_nothing_named_is_no_background_at_all(self):
        """The frame charter shipped before any of this, unchanged: `_chrome_argvs` then
        emits `_CHROME_STYLE` itself and the rule is the terminal's own."""
        self.assertIsNone(instance.border_bg({}, "off"))
        self.assertIsNone(instance.border_bg(_frame("blue", "red"), "off"))

    def test_a_colour_does_not_need_chrome_to_be_on(self):
        """`_surface_argvs`' rule, one option over: a `bg` is not a default, it is a line
        somebody wrote by hand about one pane. A plane that painted every panel and left
        `chrome = "off"` gets rules that match its panels."""
        self.assertEqual(instance.border_bg(_frame("blue", "blue"), "off"), "bg=blue")

    def test_the_word_default_is_carried_as_the_terminal_s_own(self):
        """`default` is the seventeenth word and not a colour. Panels that all opted out
        of the frame's surface get a rule that opted out with them."""
        self.assertEqual(instance.border_bg(_frame("default", "default"), "dark"),
                         "bg=default")

    def test_a_word_charter_does_not_know_falls_back_and_never_reaches_the_answer(self):
        """The containment, on this key. A committed `bg` is only ever a KEY into
        `FRAME_PANE_BG`; a word charter does not know indexes nothing, so that component
        resolves to the frame-wide surface exactly as a component with no `bg` does."""
        for hostile in ("bg=#{?#{==:1,1},colour196,colour46}", "colour236", "chartreuse",
                        "", None, ["blue"], {"a": 1}, 3, True):
            with self.subTest(value=hostile):
                self.assertEqual(instance.border_bg(_frame(hostile, None), "dark"),
                                 "bg=black")

    def test_every_answer_is_a_value_out_of_charters_own_two_tables(self):
        """Not "no `#` in it" — the stronger form: what comes back is a value charter
        wrote, identically to `chrome_options` and `pane_bg_options`, so nothing assembled
        from an operator's word can leave through here whatever the word was."""
        ours = {v for pairs in instance.FRAME_PANE_BG.values() for _n, v in pairs}
        ours |= {v for pairs in instance.FRAME_CHROME.values() for _n, v in pairs}
        for level in (*instance.FRAME_CHROME, "bg=red", None):
            for word in (*instance.FRAME_PANE_BG, "colour236", "#00ff00", None):
                with self.subTest(level=level, bg=word):
                    got = instance.border_bg(_frame(word, word), level)
                    self.assertTrue(got is None or got in ours, got)

    def test_the_rule_names_a_palette_slot_and_never_an_index_or_a_triple(self):
        """`FRAME_PANE_COLOURS`' rule reaching the border: sixteen names and `default`,
        because a colour resolves against the OPERATOR's own palette. `colour24` is a
        fixed point in the xterm cube that no theme moves, and a committed one is a
        black-on-black rule on somebody else's machine."""
        allowed = {f"bg={n}" for n in instance.FRAME_PANE_BG} | {"bg=default"}
        for level in instance.FRAME_CHROME:
            for word in instance.FRAME_PANE_BG:
                with self.subTest(level=level, bg=word):
                    got = instance.border_bg(_frame(word, word), level)
                    self.assertIn(got, allowed)
                    self.assertNotRegex(got, r"colour\d|#[0-9a-fA-F]{6}|\d{1,3},\d")

    def test_it_reads_the_arrangement_and_never_a_pane_map(self):
        """The universe is what the plane WROTE, not what is on screen — which is what
        makes the launch path and `cmd_chrome` answer identically without either of them
        holding a pane map (#610's disagreement, removed by construction), and what stops
        the rules changing colour when a toggle key hides a panel or a narrow terminal
        drops one."""
        frame = _frame(*["brightblack"] * 4, slots=["s0"])
        self.assertEqual(instance.border_bg(frame, "dark"), "bg=brightblack")
        self.assertEqual(instance.border_bg({**frame, "slots": []}, "dark"),
                         "bg=brightblack")

    def test_this_planes_own_committed_frame_answers_the_report(self):
        """The operator's charter.toml, asked directly. Four components, one word, and
        their own comment for it: "charter's chrome is grey, the work area is the
        terminal's own". Grey is `brightblack`, and until this key existed the rules
        between those grey panes were the terminal's black."""
        if not (config.FRAME.get("components") or ()):
            self.skipTest("this plane's charter.toml writes no arrangement")
        self.assertEqual(instance.border_bg(config.FRAME, config.FRAME.get("chrome")),
                         "bg=brightblack")


class TheRuleIsDrawnInIt(unittest.TestCase):
    """`commands_frame._chrome_argvs` — the five `set-option -w`s, with the surface in
    the two that are styles."""

    def _tails(self, surface=None):
        return [a[a.index("set-option"):]
                for a in commands_frame._chrome_argvs(socket="s", harness_pane="%1",
                                                      surface=surface)]

    def _values(self, surface=None):
        return {a[-2]: a[-1] for a in self._tails(surface)}

    def test_no_surface_is_byte_identical_to_the_frame_charter_shipped(self):
        """The `off` invariant, and the reason the border needs no `-u` where the pane
        surface does: these two options are charter's own at EVERY level (#514 pins them
        with or without a surface), so "no surface" is a value rather than an absence, and
        it is the value that was there before this parameter existed."""
        self.assertEqual(self._values(), dict(commands_frame._CHROME))

    def test_the_surface_is_appended_to_the_styles_and_to_nothing_else(self):
        got = self._values("bg=brightblack")
        for name in _STYLES:
            with self.subTest(option=name):
                self.assertEqual(got[name],
                                 f"{commands_frame._CHROME_STYLE},bg=brightblack")
        for name, value in commands_frame._CHROME:
            if name not in _STYLES:
                with self.subTest(option=name):
                    self.assertEqual(got[name], value,
                                     "a background was appended to an option that is "
                                     "not a style")

    def test_both_styles_get_it_or_neither_does(self):
        """#514, restated where it would break. That defect was a rule that changed
        colour where it ran past the ACTIVE pane's corner; a BACKGROUND that changed there
        is the same defect an order of magnitude more visible. tmux's own
        `pane-active-border-style` default is the format expression
        `#{?pane_in_mode,fg=yellow,#{?synchronize-panes,fg=red,fg=green}}` (measured on
        3.7c) — charter has replaced it since #514, and focus is drawn where it cannot run
        past a corner, on the pane's own rectangle by `window-active-style`."""
        for surface in (None, "bg=brightblack", "bg=default"):
            with self.subTest(surface=surface):
                got = self._values(surface)
                self.assertEqual(got["pane-border-style"],
                                 got["pane-active-border-style"])

    def test_which_options_carry_it_is_derived_from_the_table(self):
        """`instance.chrome_option_names`' discipline said about `_CHROME`: the options a
        surface belongs in are the ones already pinned to `_CHROME_STYLE`, so a sixth
        border-style option added to that table is not the one rule left with a seam
        through it."""
        with mock.patch.object(
                commands_frame, "_CHROME",
                commands_frame._CHROME + (("pane-border-status-style",
                                           commands_frame._CHROME_STYLE),)):
            self.assertEqual(self._values("bg=blue")["pane-border-status-style"],
                             f"{commands_frame._CHROME_STYLE},bg=blue")

    def test_the_option_names_and_the_scope_are_unchanged(self):
        """Every one of these is set `-w` and targets a pane only to name its WINDOW.
        Nothing is written on the pane itself, which is how ADR 0018's boundary holds here
        by construction rather than by care."""
        for tail in self._tails("bg=blue"):
            with self.subTest(argv=tail):
                self.assertEqual(tail[:4], ["set-option", "-w", "-t", "%1"])

    def test_no_colour_drops_the_surface_and_keeps_the_rules(self):
        """The split that matters: `_CHROME_STYLE` is an attribute over the terminal's own
        foreground and never a colour charter chose, so it is not what `NO_COLOR` is
        about — a background is exactly what it is about, and charter asking tmux to paint
        one is still charter putting colour on their screen."""
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}, clear=True):
            self.assertEqual(self._values("bg=brightblack"),
                             dict(commands_frame._CHROME))
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertNotEqual(self._values("bg=brightblack"),
                                dict(commands_frame._CHROME))

    def test_no_operator_string_reaches_a_style_value(self):
        """The boundary asserted end to end rather than at the table: whatever a committed
        `bg` says, the value that reaches a `set-option` is `_CHROME_STYLE` plus a clause
        charter wrote. A tmux style is format-expanded at draw time — measured on this
        very option: `set -w pane-border-style 'bg=#{?#{==:1,1},colour196,colour46}'` is
        rc 0 and reads back verbatim — so this cannot be tmux's job."""
        hostile = "bg=#{?#{==:1,1},colour196,colour46}"
        for level in ("dark", hostile):
            argvs = commands_frame._chrome_argvs(
                socket="s", harness_pane="%1",
                surface=instance.border_bg(_frame(hostile, hostile), level))
            for argv in argvs:
                for word in argv:
                    self.assertNotIn("#", word)


class APanelDrawsItsOwnEdgesAndTheHarnessKeepsItsOwn(unittest.TestCase):
    """#631's half that stands. `instance.pane_border_options` and
    `commands_frame._pane_border_pairs`.

    #628 closed the seam by putting the surface on the WINDOW's two border options, which
    is ONE value for every rule in the frame — so a panel whose colour is not the frame's
    got rules in a colour it does not wear. Above `tmuxctl.PANE_BORDER_FLOOR` those options
    are PANE options, so each panel carries its own.

    What #631 also did — leave the harness's two unset — is undone by
    `TheRulesAroundTheHarnessAreTheFramesToo` below, and the reason is in this table:
    the harness's edge cells are drawn from the HARNESS's options, so unset meant unpainted
    on one end of a rule whose other end was painted. Measured on 3.7c through a nested
    client, charter's real four-panel shape at 100x24::

        window-wide (#628)  harness top ESC[100m  right ESC[100m  panel|panel ESC[100m
        per pane    (#631)  harness top ESC[49m   right ESC[49m   panel|panel ESC[100m
        with the rules      harness top ESC[100m  right ESC[100m  panel|panel ESC[100m
    """

    STYLE = "fg=default,dim"

    def test_a_pane_with_a_colour_gets_both_of_its_edges_in_that_colour(self):
        self.assertEqual(
            instance.pane_border_options("brightblack", "dark", self.STYLE),
            (("pane-border-style", "fg=default,dim,bg=brightblack"),
             ("pane-active-border-style", "fg=default,dim,bg=brightblack")))

    def test_a_pane_with_no_colour_of_its_own_takes_the_frame_wide_one(self):
        """The same `pane_bg_options(bg) or chrome_options(chrome)` its INTERIOR is
        painted from, so a pane and its edges cannot come out two colours."""
        self.assertEqual(
            [v for _n, v in instance.pane_border_options(None, "dark", self.STYLE)],
            [f"{self.STYLE},{dict(instance.FRAME_CHROME['dark'])['window-style']}"] * 2)

    def test_a_pane_with_no_surface_at_all_sets_nothing(self):
        """Not a bare style — the window option already IS that. A pane-scoped copy of the
        window's own value would be a value `off`'s removal has to find and remove, for a
        pane that never needed one."""
        self.assertEqual(instance.pane_border_options(None, "off", self.STYLE), ())
        self.assertEqual(instance.pane_border_options("nonsense", "off", self.STYLE), ())

    def test_both_edges_always_carry_the_identical_value(self):
        """#514 surviving the move to pane scope. tmux picks between the two per border
        CELL — the active one for a cell touching the active pane — so a pane whose two
        differed would have edges that changed colour at the active pane's corner."""
        for word in (None, *instance.FRAME_PANE_BG):
            for level in instance.FRAME_CHROME:
                with self.subTest(bg=word, chrome=level):
                    values = {v for _n, v
                              in instance.pane_border_options(word, level, self.STYLE)}
                    self.assertLessEqual(len(values), 1, values)

    def test_the_two_option_names_are_the_ones_the_removal_reads(self):
        """One list, read by the setter and by `_resurface_argvs`' unset — so an option
        added to the pair cannot be set by one half and left behind by the other."""
        self.assertEqual(
            tuple(n for n, _v in instance.pane_border_options("blue", "dark", self.STYLE)),
            instance.PANE_BORDER_OPTIONS)

    def test_no_operator_string_reaches_a_pane_edge(self):
        """The containment, on the new key: the value is charter's style constant plus a
        `window-style` value out of charter's own tables, indexed by a word that was only
        ever a key."""
        ours = {v for pairs in instance.FRAME_PANE_BG.values() for _n, v in pairs}
        ours |= {v for pairs in instance.FRAME_CHROME.values() for _n, v in pairs}
        for word in (*instance.FRAME_PANE_BG, "colour236", "chartreuse", None, 7,
                     "bg=#{?#{==:1,1},colour196,colour46}", ["blue"], True):
            with self.subTest(bg=word):
                for _n, value in instance.pane_border_options(word, "dark", self.STYLE):
                    self.assertNotIn("#", value)
                    self.assertIn(value.removeprefix(f"{self.STYLE},"), ours)


class TheRulesAroundTheHarnessAreTheFramesToo(unittest.TestCase):
    """The seam #631 left. `instance.agreed_border_bg`, `instance.rule_options` and
    `commands_frame._harness_rule_argvs`.

    **The report, for the third time and off a screenshot each time.** Above
    `tmuxctl.PANE_BORDER_FLOOR` tmux resolves every border cell against exactly one pane —
    `screen_redraw_check_cell` takes the first pane in the window's list whose border box
    contains the cell — and the harness is the first pane charter's window has. So its top,
    right and bottom rules are all drawn from the HARNESS's two options, and #631 leaving
    them unset did not leave "the harness's own edges dark": it left one rule dark for the
    cells over the harness and surfaced for the cells over the sidebar. Rendered through a
    nested client, charter's real four-panel shape at 100x24, every panel
    `bg = "brightblack"`::

        #631        row 1: cols 0-77 ESC[49m  cols 78-99 ESC[100m   <- one rule, two colours
        with this   row 1: cols 0-99 ESC[100m                       <- one rule, one colour

    which is #514's own defect — a rule that changes colour where it passes a corner — in a
    place #514 never looked.

    **What #631 was actually right about is one option name over.** The harness's INTERIOR
    is `window-style`, and nothing here can reach it: these values are built from
    `instance.PANE_BORDER_OPTIONS`, and `_surface_argvs`, which does set the interior, is
    still only ever handed a panel. That is asserted below rather than argued, because it
    is the whole of what naming the harness here costs.
    """

    STYLE = commands_frame._CHROME_STYLE

    def _surface_of(self, word, level):
        """What a PANEL with this component `bg` at this level actually wears — the
        expression `_surface_argvs` paints an interior from, so "the harness's rules match a
        pane" is asked about the colour on the screen rather than the word in the file."""
        return dict(instance.pane_bg_options(word)
                    or instance.chrome_options(level)).get("window-style")

    def test_the_harness_rules_take_the_colour_every_panel_agrees_on(self):
        self.assertEqual(instance.agreed_border_bg(_frame(*["brightblack"] * 4), "dark"),
                         "bg=brightblack")

    def test_panels_that_disagree_leave_the_harness_rules_bare(self):
        """The one refusal in the function, and the reason it is not `border_bg`. The
        harness's three edges have three different panels on the far side of them; where
        those wear more than one colour, no single value matches all three, and a
        compromise would be a cell matching NEITHER pane beside it — which is the seam
        #627 reported, moved."""
        self.assertIsNone(instance.agreed_border_bg(_frame("blue", "brightblack"), "dark"))
        self.assertIsNone(instance.agreed_border_bg(_frame("brightblack", None), "light"))

    def test_that_refusal_is_the_whole_difference_from_the_frame_wide_answer(self):
        """The control for the test above: `border_bg` answers the same disagreement with
        the frame-wide colour and is still right to, because below the floor one value for
        every rule is all tmux can be told. A test that only said "None" here would pass
        just as well against a function that always did."""
        frame = _frame("blue", "brightblack")
        self.assertEqual(instance.border_bg(frame, "dark"),
                         dict(instance.FRAME_CHROME["dark"])["window-style"])
        self.assertIsNone(instance.agreed_border_bg(frame, "dark"))

    def test_the_harness_rule_is_never_a_colour_no_panel_wears(self):
        """**The invariant, asked of every arrangement three components can make.** Every
        rule cell must wear the surface of a pane it TOUCHES: a panel's edges take that
        panel's colour and so always match their own side, and the harness's edges have a
        pane charter never paints on one side, so they must match the panel on the other.
        Where this answers a colour at all, that colour is what every panel wears."""
        answered = 0
        for level in instance.FRAME_CHROME:
            for words in ((None, None, None), ("brightblack",) * 3, ("blue",) * 3,
                          ("blue", "brightblack", None), (None, None, "blue"),
                          ("blue", "blue", None), ("nonsense", "nonsense", "nonsense")):
                with self.subTest(chrome=level, bgs=words):
                    surface = instance.agreed_border_bg(_frame(*words), level)
                    if surface is None:
                        continue
                    answered += 1
                    self.assertEqual({self._surface_of(w, level) for w in words},
                                     {surface})
        # Without this the whole loop above is satisfied by a function that answers `None`
        # to everything, which is the fourth flavour of test-that-cannot-fail this repo
        # has caught. Half the arrangements here name one colour and must get one.
        self.assertGreaterEqual(answered, 8,
                                "almost nothing was answered a colour at all, so the "
                                "invariant above was asserted of nearly nothing")

    def test_a_plane_that_wrote_no_component_agrees_on_the_frame_wide_colour(self):
        """Empty is not disagreement: every panel of a plane spelled with `slots` takes the
        frame-wide surface, so they agree on it. `border_bg`'s own answer, and it must stay
        the same one or the two paths would draw one frame two ways."""
        for level in instance.FRAME_CHROME:
            with self.subTest(chrome=level):
                self.assertEqual(instance.agreed_border_bg({"slots": ["top"]}, level),
                                 instance.border_bg({"slots": ["top"]}, level))
        self.assertEqual(instance.agreed_border_bg({}, "dark"),
                         dict(instance.FRAME_CHROME["dark"])["window-style"])

    def test_off_with_no_colour_anywhere_is_no_surface_at_all(self):
        self.assertIsNone(instance.agreed_border_bg(_frame(None, None), "off"))

    def test_a_component_colour_survives_off_here_as_it_does_everywhere(self):
        """`off` removes the FRAME's surface and never a colour a component wrote, so the
        rules round the harness keep it — the same rule one key over."""
        self.assertEqual(instance.agreed_border_bg(_frame("blue", "blue"), "off"),
                         "bg=blue")

    def test_the_harness_rules_are_byte_identical_to_a_panels(self):
        """The reason `instance.rule_options` is one function and not an expression in
        two: when the panels agree, the value on the harness and the value on each panel
        are assembled by the same code from the same surface, so #514's "one rule, one
        colour" cannot be lost to a spelling."""
        for word in ("brightblack", "blue"):
            with self.subTest(bg=word):
                self.assertEqual(
                    instance.rule_options(
                        instance.agreed_border_bg(_frame(*[word] * 4), "dark"), self.STYLE),
                    instance.pane_border_options(word, "dark", self.STYLE))

    def test_both_of_the_harness_options_carry_the_identical_value(self):
        """#514, on the pane where it is most visible: a rule whose two options differed
        would change colour where it passed the active pane's corner."""
        pairs = instance.rule_options("bg=blue", self.STYLE)
        self.assertEqual(tuple(n for n, _v in pairs), instance.PANE_BORDER_OPTIONS)
        self.assertEqual({v for _n, v in pairs}, {f"{self.STYLE},bg=blue"})

    def test_no_surface_builds_no_pair_at_all(self):
        self.assertEqual(instance.rule_options(None, self.STYLE), ())

    def test_no_operator_string_reaches_the_rules_around_the_harness(self):
        """The containment, on the new path: what comes back is charter's style constant
        plus a `window-style` value out of charter's own tables, indexed by a word that was
        only ever a key."""
        ours = {v for pairs in instance.FRAME_PANE_BG.values() for _n, v in pairs}
        ours |= {v for pairs in instance.FRAME_CHROME.values() for _n, v in pairs}
        for word in (*instance.FRAME_PANE_BG, "colour236", "chartreuse", None, 7,
                     "bg=#{?#{==:1,1},colour196,colour46}", ["blue"], True):
            with self.subTest(bg=word):
                surface = instance.agreed_border_bg(_frame(word, word), "dark")
                pairs = instance.rule_options(surface, self.STYLE)
                self.assertTrue(pairs, "no value was built at all, so nothing was "
                                       "contained here")
                for _n, value in pairs:
                    self.assertNotIn("#", value)
                    self.assertIn(value.removeprefix(f"{self.STYLE},"), ours)

    def _argvs(self, **kw):
        return commands_frame._harness_rule_argvs(socket="s", harness_pane="%1", **kw)

    def test_above_the_floor_both_options_are_set_on_the_harness_pane_itself(self):
        """`-p`, and `-t <the harness>` — the one place charter targets that pane with a
        pane-scoped write. The value is asserted, not the fact of a command."""
        self.assertEqual(
            [a[a.index("set-option"):] for a in
             self._argvs(surface="bg=brightblack", pane_borders=True)],
            [["set-option", "-p", "-t", "%1", n, f"{self.STYLE},bg=brightblack"]
             for n in instance.PANE_BORDER_OPTIONS])

    def test_no_surface_is_the_unset_and_never_silence(self):
        """The harness pane is not fresh on either path that reaches here — `_split_panels`
        runs again on every density change and `cmd_chrome` on a frame surfaced hours ago —
        so "no surface" has to REMOVE what is there, or a plane that changed its colours
        keeps yesterday's rule round the one pane it is most visible on."""
        self.assertEqual(
            [a[a.index("set-option"):] for a in
             self._argvs(surface=None, pane_borders=True)],
            [["set-option", "-p", "-u", "-t", "%1", n]
             for n in instance.PANE_BORDER_OPTIONS])

    def test_below_the_floor_nothing_at_all_is_written_on_the_harness(self):
        """Neither half. `set -p` is rc 0 and writes the WINDOW there, so a set would let
        the harness decide every rule in the frame and a `-u` would remove charter's own
        #514 pin from all of them — silently, with nothing for `tmuxctl.run` to report.
        `_chrome_argvs` carries the frame-wide answer instead, and it already reaches
        these cells."""
        for v in (None, (3, 2), (3, 6)):
            for surface in ("bg=brightblack", None):
                with self.subTest(v=v, surface=surface):
                    self.assertEqual(
                        self._argvs(surface=surface,
                                    pane_borders=commands_frame._pane_borders_wanted(v)),
                        [])

    def test_no_colour_takes_the_surface_off_the_harness_rules_too(self):
        """And on this path that means the unsets rather than silence: a frame surfaced
        before `NO_COLOR` was exported has a value on this pane to remove."""
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}, clear=True):
            self.assertEqual(
                [a[a.index("set-option"):] for a in
                 self._argvs(surface="bg=brightblack", pane_borders=True)],
                [["set-option", "-p", "-u", "-t", "%1", n]
                 for n in instance.PANE_BORDER_OPTIONS])

    def test_nothing_here_can_name_an_option_that_paints_an_interior(self):
        """**ADR 0018's real boundary, and the whole of what naming the harness costs.**
        The rectangle charter never draws in is painted by `window-style` and
        `window-active-style`; neither is in `instance.PANE_BORDER_OPTIONS`, so no surface
        this is handed — charter's own or otherwise — can produce one."""
        for surface in ("bg=brightblack", "bg=blue", None):
            with self.subTest(surface=surface):
                for argv in self._argvs(surface=surface, pane_borders=True):
                    self.assertNotIn("window-style", argv)
                    self.assertNotIn("window-active-style", argv)
        self.assertEqual(
            set(instance.PANE_BORDER_OPTIONS) & set(instance.chrome_option_names()), set(),
            "an interior option reached the list the harness's rules are built from")


class TheFloorCannotHavePerPaneEdgesAndIsNotGivenThem(unittest.TestCase):
    """`commands_frame._pane_borders_wanted` and `tmuxctl.PANE_BORDER_FLOOR`.

    **The gate is on the WRITE, not on the value, and the reason is that the floor fails
    silently.** A refused option is loud and already handled — `tmuxctl.run` reports it and
    the launch continues. Below the floor `set -p pane-border-style` is **rc 0 and writes
    the WINDOW** (measured against a real 3.2 built from source on this machine): the last
    panel written would decide every rule in the frame, and an `off` would `set -p -u`
    charter's own #514 pin away for the whole window.
    """

    def test_the_floor_is_the_version_the_option_gained_pane_scope_in(self):
        """Read out of tmux's own `options-table.c` at every release either side of the
        line — `OPTIONS_TABLE_WINDOW` in 3.2 through 3.6a, `…|OPTIONS_TABLE_PANE` from 3.7
        — and run on both sides. Pinned so a floor moved without a measurement is red."""
        self.assertEqual(tmuxctl.PANE_BORDER_FLOOR, (3, 7))
        self.assertGreater(tmuxctl.PANE_BORDER_FLOOR, tmuxctl.FLOOR,
                           "an operator charter still lets launch would now be issuing "
                           "pane-scoped writes their tmux silently applies to the window")

    def test_only_a_version_at_or_above_the_floor_gets_its_own_edges(self):
        for v, want in ((None, False), ((3, 2), False), ((3, 6), False),
                        ((3, 7), True), ((3, 9), True), ((4, 0), True)):
            with self.subTest(v=v):
                self.assertIs(commands_frame._pane_borders_wanted(v), want)

    def test_an_unreadable_version_takes_the_design_that_cannot_be_wrong(self):
        """`None` is "charter could not find out", and it answers the frame-wide design —
        correct on every tmux — rather than the per-pane one, which is correct only above
        the floor and damaging below it."""
        self.assertIs(commands_frame._pane_borders_wanted(None), False)

    def test_below_the_floor_no_border_option_is_ever_set_on_a_pane(self):
        for v in (None, (3, 2), (3, 6)):
            with self.subTest(v=v):
                self.assertEqual(
                    commands_frame._pane_border_pairs(
                        chrome="dark", bg="brightblack",
                        pane_borders=commands_frame._pane_borders_wanted(v)), ())

    def test_below_the_floor_no_border_option_is_ever_UNSET_on_a_pane(self):
        """The half that would be silent damage rather than a missing colour: `set -p -u`
        on these names removes the WINDOW's value below the floor, and the window's value
        is charter's #514 pin for every rule in the frame."""
        for v in (None, (3, 2), (3, 6)):
            argvs = commands_frame._resurface_argvs(
                socket="s", pane_id="%3", chrome="off",
                pane_borders=commands_frame._pane_borders_wanted(v))
            with self.subTest(v=v):
                for name in instance.PANE_BORDER_OPTIONS:
                    self.assertNotIn(name, [a[-1] for a in argvs])

    def test_above_the_floor_off_removes_the_edges_it_set(self):
        argvs = commands_frame._resurface_argvs(socket="s", pane_id="%3", chrome="off",
                                                pane_borders=True)
        tails = [a[a.index("set-option"):] for a in argvs]
        for name in instance.PANE_BORDER_OPTIONS:
            with self.subTest(option=name):
                self.assertIn(["set-option", "-p", "-u", "-t", "%3", name], tails)

    def test_a_pane_that_keeps_its_colour_keeps_its_edges(self):
        """`off` is the frame's removal and never a component's: a pane whose component
        named a colour has that colour SET here, edges included, and nothing unset."""
        argvs = commands_frame._resurface_argvs(socket="s", pane_id="%3", chrome="off",
                                                bg="brightblack", pane_borders=True)
        self.assertEqual([a for a in argvs if "-u" in a], [])
        self.assertIn("fg=default,dim,bg=brightblack", [a[-1] for a in argvs])

    def test_no_colour_takes_the_edges_off_too(self):
        """The fill is tmux's paint either way, so a `NO_COLOR` that stripped the pane and
        left its edges would be charter having asked somebody else to paint them."""
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}, clear=True):
            self.assertEqual(
                commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="dark",
                                              bg="brightblack", pane_borders=True), [])
            tails = [a[a.index("set-option"):] for a in commands_frame._resurface_argvs(
                socket="s", pane_id="%3", chrome="dark", bg="brightblack",
                pane_borders=True)]
            for name in instance.PANE_BORDER_OPTIONS:
                self.assertIn(["set-option", "-p", "-u", "-t", "%3", name], tails)

    def test_the_edges_are_set_on_the_pane_and_never_on_the_window(self):
        """`-p`, which is what keeps them off the harness: `_surface_argvs` is only ever
        handed a PANEL pane, so ADR 0018's boundary carries the new options unchanged."""
        for argv in commands_frame._surface_argvs(socket="s", pane_id="%3", chrome="dark",
                                                  bg="brightblack", pane_borders=True):
            with self.subTest(argv=argv):
                self.assertIn("-p", argv)
                self.assertNotIn("-w", argv)


class TheLauncherActuallyArmsTheRulesWithIt(PersonaIso, unittest.TestCase):
    """**The wiring, which every `_chrome_argvs` test above takes as given.**

    `TheLauncherActuallyAsksForEachPanesColour` in `tests/test_frame_pane_style.py` was
    written because a hand-check replaced `_split_panels`' one resolving line with
    ``bg = None`` and the whole suite stayed green — a per-pane colour that production
    never resolves is a feature that does nothing on a real frame and a test file that
    says it works. The *surface* argument has exactly that shape, so it is driven through
    exactly that funnel: `_split_panels`, which both launch paths and every density change
    come through, with only `tmuxctl.run` faked.
    """

    def _issued(self, frame: dict, slots: list[str], *, fid="f-1",
                v=None) -> list[list[str]]:
        seen: list[list[str]] = []
        panes = iter(f"%{n}" for n in range(10, 99))

        def fake_run(_why, argv, **_kw):
            seen.append(list(argv))
            out = next(panes) if "split-window" in argv else ""
            return subprocess.CompletedProcess(argv, 0, stdout=out + "\n", stderr="")

        with mock.patch.dict(config.FRAME, frame), \
                mock.patch.object(commands_frame.tmuxctl, "run", fake_run), \
                mock.patch.dict(os.environ, {}, clear=True):
            commands_frame._split_panels("sock", slots=slots, fid=fid,
                                         harness_pane="%1", env=None, pane_env=None, v=v)
        return seen

    def _rules(self, issued: list[list[str]]) -> dict[str, str]:
        return {a[-2]: a[-1] for a in issued
                if "set-option" in a and "-w" in a and a[-2] in dict(commands_frame._CHROME)}

    def test_a_launch_below_the_floor_draws_its_rules_in_the_agreed_colour(self):
        """The frame-wide design, which is all a tmux without pane-scoped border options
        can be told. `v=None` is the default here and answers it, which is the safe
        direction (`_pane_borders_wanted`)."""
        frame = _resolved(*["brightblack"] * 4, chrome="dark")
        rules = self._rules(self._issued(frame, ["top", "bottom", "repos", "right"]))
        self.assertEqual(rules["pane-border-style"],
                         f"{commands_frame._CHROME_STYLE},bg=brightblack")
        self.assertEqual(rules["pane-active-border-style"], rules["pane-border-style"])

    def _per_pane(self, issued, names) -> dict[str, list[str]]:
        out: dict[str, list[str]] = {}
        for a in issued:
            if "set-option" in a and "-p" in a and "-u" not in a and a[-2] in names:
                out.setdefault(a[a.index("-t") + 1], []).append(a[-1])
        return out

    def test_a_launch_above_the_floor_gives_each_panel_its_own_edges(self):
        """#631's wiring, through the same funnel: the WINDOW keeps charter's bare #514
        style and every panel pane that was actually split gets its own two options."""
        frame = _resolved(*["brightblack"] * 4, chrome="dark")
        issued = self._issued(frame, ["top", "bottom", "repos", "right"], v=(3, 7))
        self.assertEqual(self._rules(issued), dict(commands_frame._CHROME),
                         "the window carries a surface above the floor, so a rule is "
                         "being coloured in two places")
        want = f"{commands_frame._CHROME_STYLE},bg=brightblack"
        per_pane = self._per_pane(issued, instance.PANE_BORDER_OPTIONS)
        self.assertEqual(len(per_pane), 5,
                         f"not every panel and the harness got edges: {per_pane}")
        for pane, values in per_pane.items():
            with self.subTest(pane=pane):
                self.assertEqual(values, [want, want])

    def test_the_harness_gets_the_frames_rules_and_never_the_frames_surface(self):
        """**The whole of what naming the harness here buys and costs**, driven through the
        funnel rather than asserted of the builder in isolation.

        Buys: the three rules AROUND the harness carry the same value every panel's do, so
        a horizontal rule running from over the harness to over the sidebar is one colour.
        Costs: nothing, and that is asserted rather than argued — the two options that
        paint a pane's INTERIOR are never written on that pane at any version, which is the
        rectangle ADR 0018 is about.
        """
        frame = _resolved(*["brightblack"] * 4, chrome="dark")
        want = f"{commands_frame._CHROME_STYLE},bg=brightblack"
        for v in (None, (3, 2), (3, 7)):
            with self.subTest(v=v):
                issued = self._issued(frame, ["top", "right"], v=v)
                edges = self._per_pane(issued, instance.PANE_BORDER_OPTIONS)
                self.assertEqual(edges.get("%1"), [want, want] if v == (3, 7) else None,
                                 "the rules round the harness took a different answer "
                                 "from the rest of the frame")
                interiors = self._per_pane(issued, instance.chrome_option_names())
                self.assertNotIn("%1", interiors,
                                 "charter painted inside the harness pane")
                self.assertTrue(interiors, "no pane was painted at all, so the assertion "
                                           "above proves nothing")

    def test_a_launch_that_names_no_colour_arms_exactly_what_it_always_did(self):
        for v in (None, (3, 2), (3, 7)):
            with self.subTest(v=v):
                issued = self._issued({}, ["top", "bottom"], v=v)
                self.assertEqual(self._rules(issued), dict(commands_frame._CHROME))
                self.assertEqual([a for a in issued if "set-option" in a and "-p" in a
                                  and a[-2] in instance.PANE_BORDER_OPTIONS], [])

    def test_a_relayout_that_adds_one_pane_still_arms_the_WHOLE_frames_surface(self):
        """`_relayout` calls this funnel with only the slots being ADDED, so a rule colour
        derived from the *slots* argument would change every time a density level brought
        a panel back — and would be wrong for the panels already on screen. It is derived
        from `config.FRAME` instead, which is the same thing `cmd_chrome` reads."""
        frame = _resolved(*["blue"] * 4, chrome="dark")
        whole = self._rules(self._issued(frame, ["top", "bottom", "repos", "right"]))
        one = self._rules(self._issued(frame, ["right"]))
        self.assertEqual(one, whole)
        self.assertEqual(one, whole)
        self.assertEqual(one["pane-border-style"],
                         f"{commands_frame._CHROME_STYLE},bg=blue")

    def test_the_live_word_and_not_the_committed_one_decides_the_rules(self):
        """`_split_panels` resolves the level through `_current_chrome`, so a pane split
        into a frame the operator has since surfaced from the palette is bordered with the
        surface the frame IS rather than the one it launched with — the same resolver the
        pane's own background already goes through."""
        frame = _resolved(None, None, None, None, chrome="off")
        state.record_chrome("f-live", "dark")
        rules = self._rules(self._issued(frame, ["top"], fid="f-live"))
        self.assertEqual(rules["pane-border-style"],
                         f"{commands_frame._CHROME_STYLE},"
                         f"{dict(instance.FRAME_CHROME['dark'])['window-style']}")


class TheVersionReachesTheFunnelFromTheFunctionThatKnowsIt(PersonaIso, unittest.TestCase):
    """**The deletion sweep found this one and it was a real hole.** `_split_panels` takes
    the tmux version as an argument, and every other test here hands it one directly — so
    replacing `_draw_panels`' own ``v=v`` with ``v=None`` left the whole suite green while
    a real frame on a modern tmux silently took the FLOOR design and put the box back round
    the harness. A per-pane edge production never asks for is a fix that does nothing on a
    real frame and a test file that says it works, which is what
    `TheLauncherActuallyAsksForEachPanesColour` in `tests/test_frame_pane_style.py` exists
    for one key over.

    `_draw_panels` and not `cmd_launch`, and that is a finding rather than a shortcut:
    `tests/test_frame_launcher.py`'s fake reports no pane id for a `split-window`, so a
    whole faked launch never reaches the per-pane surface AT ALL — `_surface_argvs` is not
    called once. That is why the pane-style tests drive this function too, and a test built
    on the launcher's fake would have passed while asserting nothing.
    """

    def _calls(self, v):
        seen: list[list[str]] = []
        panes = iter(f"%{n}" for n in range(20, 99))

        def fake_run(_why, argv, **_kw):
            seen.append(list(argv))
            out = next(panes) if "split-window" in argv else ""
            return subprocess.CompletedProcess(argv, 0, stdout=out + "\n", stderr="")

        frame = _resolved(*["brightblack"] * 4, chrome="dark")
        with mock.patch.object(config, "FRAME", frame), \
                mock.patch.object(commands_frame.tmuxctl, "run", fake_run), \
                mock.patch.dict(os.environ, {}, clear=True):
            commands_frame._draw_panels("s", slots=["top", "right"], fid="f-v",
                                        harness_pane="%1", env=None, v=v)
        return seen

    def _edges(self, calls):
        return {a[a.index("-t") + 1]: a[-1] for a in calls
                if "set-option" in a and "-p" in a and a[-2] == "pane-border-style"}

    def _window_rule(self, calls):
        return next(a[-1] for a in calls
                    if "set-option" in a and "-w" in a and a[-2] == "pane-border-style")

    def test_a_frame_drawn_on_a_modern_tmux_really_does_get_per_pane_edges(self):
        calls = self._calls((3, 7))
        edges = self._edges(calls)
        self.assertEqual(len(edges), 3,
                         "the version never reached `_split_panels`, so a real frame "
                         f"above the floor took the floor's design: {edges}")
        self.assertEqual(set(edges.values()),
                         {f"{commands_frame._CHROME_STYLE},bg=brightblack"})
        self.assertEqual(self._window_rule(calls), commands_frame._CHROME_STYLE,
                         "the window kept a surface above the floor, so a rule is being "
                         "coloured in two places")

    def test_a_frame_drawn_on_an_old_tmux_really_does_fall_back_to_the_window(self):
        calls = self._calls((3, 6))
        self.assertEqual(self._edges(calls), {},
                         "a frame below the floor wrote a pane-scoped border option, "
                         "which on that tmux lands on the window")
        self.assertEqual(self._window_rule(calls),
                         f"{commands_frame._CHROME_STYLE},bg=brightblack")

    def test_the_harness_is_among_the_panes_given_edges_only_above_the_floor(self):
        """The version has to reach the harness's own rules too, and the direction it can
        be wrong in is not symmetric: missing above the floor is the seam this closed, and
        present below it is a pane-scoped write that lands on the WINDOW and takes charter's
        #514 pin with it."""
        self.assertNotIn("%1", self._edges(self._calls((3, 6))))
        self.assertEqual(self._edges(self._calls((3, 7))).get("%1"),
                         f"{commands_frame._CHROME_STYLE},bg=brightblack",
                         "the rules round the harness are bare while the panels' are "
                         "surfaced, so one rule is two colours")

    def test_the_harness_pane_is_never_among_the_panes_given_a_SURFACE(self):
        """ADR 0018's boundary, which the rules above do not touch: the pane every split
        TARGETS is still never the pane an INTERIOR is painted on, at either version."""
        for v in ((3, 6), (3, 7)):
            with self.subTest(v=v):
                painted = {a[a.index("-t") + 1] for a in self._calls(v)
                           if "set-option" in a and "-p" in a
                           and a[-2] in instance.chrome_option_names()}
                self.assertNotIn("%1", painted,
                                 "charter painted inside the harness pane")
                self.assertTrue(painted, "no pane was painted at all")


class TheLaunchPathAndTheLivePathAgree(PersonaIso, unittest.TestCase):
    """#610's pin, on the border: what a frame LAUNCHES with and what a keypress leaves it
    with must be the same value, at every level and for every colour.

    The defect that rule exists for is specific and was paid for once already: a component
    with `bg = "brightblack"` repainted in the frame-wide colour by `chrome: light`, and
    ERASED by `chrome: off`, with nothing to bring it back until relaunch.
    """

    FID = "fr-border"

    def _launch_value(self, frame, level, v=None):
        return self._styles(self._launch_calls(frame, level, v))

    def _launch_calls(self, frame, level, v=None):
        """What `_split_panels` — the funnel both launch paths and every density change
        come through — actually issues, with the frame's live word recorded rather than
        the resolver stubbed out. A reconstruction of the expression would agree with the
        live path by being written twice, which is the thing #610 is about."""
        calls: list[list[str]] = []
        state.record_chrome(self.FID, level)

        def fake_run(_why, argv, **_kw):
            calls.append(list(argv))
            return subprocess.CompletedProcess(argv, 0, stdout="%9\n", stderr="")

        with mock.patch.object(config, "FRAME", frame), \
                mock.patch.object(commands_frame.tmuxctl, "run", fake_run), \
                mock.patch.dict(os.environ, {}, clear=True):
            commands_frame._split_panels("s", slots=["top"], fid=self.FID,
                                         harness_pane="%1", env=None, pane_env=None, v=v)
        return calls

    def _styles(self, calls):
        """Every border style charter set, keyed by `(scope, option)` — so a value that
        moved from the window to the pane shows up as a difference rather than as two
        tests that each still pass."""
        out = {}
        for a in calls:
            if "set-option" not in a or a[-2] not in ("pane-border-style",
                                                      "pane-active-border-style"):
                continue
            out[("-w" if "-w" in a else "-p", a[-2])] = a[-1]
        return out

    def _harness_rules(self, calls):
        """Every pane-scoped write charter made ON THE HARNESS PANE, set and unset alike,
        in the order it made them. `_styles` cannot answer this: it keys by scope and
        option, so the harness's `-p` write and a panel's collapse into one entry.

        From `set-option` on, because the two paths reach two SOCKETS by construction — a
        launch is told one and `cmd_chrome` reads the frame's own record — and that is not
        the disagreement #610 is about."""
        return [list(a[a.index("set-option"):]) for a in calls
                if "set-option" in a and "-p" in a
                and a[a.index("-t") + 1] == "%1"]

    def _live_value(self, frame, level, v=None):
        return self._styles(self._live_calls(frame, level, v))

    def _live_calls(self, frame, level, v=None):
        calls: list[list[str]] = []
        state.record_panes(self.FID, panels={"top": "%2"})
        state.record_harness_pane(self.FID, "%1")
        with mock.patch.object(config, "FRAME", frame), \
                mock.patch.object(commands_frame.tmuxctl, "run",
                                  lambda _w, argv, **kw: calls.append(argv)), \
                mock.patch.object(commands_frame.tmuxctl, "version", lambda: v), \
                mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID},
                                clear=True):
            commands_frame.cmd_chrome(type("A", (), {"level": level})())
        return calls

    def test_every_level_crossed_with_every_colour_lands_on_one_value(self):
        """And crossed with the VERSION too, since #631 made which design applies depend
        on it — a launch above the floor and a keypress below it would be two frames."""
        for v in (None, (3, 2), (3, 6), (3, 7), (4, 0)):
            for level in instance.FRAME_CHROME:
                for word in (None, *instance.FRAME_PANE_BG):
                    frame = _resolved(word, word, word, word)
                    with self.subTest(v=v, level=level, bg=word):
                        self.assertEqual(self._launch_value(frame, level, v),
                                         self._live_value(frame, level, v))

    def test_the_rules_round_the_harness_agree_across_the_two_paths_as_well(self):
        """`_styles` keys by SCOPE and option, so a harness write and a panel write of the
        same option collapse into one entry and a path that wrote only one of them would
        still match. Asked separately, of the pane itself, and of the unsets too — a level
        that removes the surface on one path and leaves it on the other is the same
        #610 disagreement wearing `-u`.

        The arrangements are crossed deliberately: one where every component agrees, and
        one where they do not and the rules round the harness must therefore come off.
        """
        for v in ((3, 2), (3, 7)):
            for level in instance.FRAME_CHROME:
                for frame in (_resolved(*["brightblack"] * 4),
                              _resolved("blue", "brightblack", None, "blue"),
                              {"slots": ["top"]}):
                    with self.subTest(v=v, level=level, frame=frame.get("components")):
                        self.assertEqual(
                            self._harness_rules(self._launch_calls(frame, level, v)),
                            self._harness_rules(self._live_calls(frame, level, v)))

    def test_a_disagreeing_arrangement_takes_the_rules_off_the_harness(self):
        """The control for the test above, which would pass on two paths that both did
        nothing. Above the floor the rules round the harness are REMOVED where the panels
        wear more than one colour — the frame-wide fallback would be a third colour on
        every one of the harness's three edges — and below it nothing pane-scoped is
        written at all."""
        frame = _resolved("blue", "brightblack", None, "blue")
        removal = [["set-option", "-p", "-u", "-t", "%1", n]
                   for n in instance.PANE_BORDER_OPTIONS]
        # Both paths, and not just the live one: a call site that reached for
        # `instance.border_bg` here would answer the frame-wide colour instead of nothing,
        # and the agreement test above would still pass with BOTH of them wrong.
        self.assertEqual(self._harness_rules(self._live_calls(frame, "dark", (3, 7))),
                         removal)
        self.assertEqual(self._harness_rules(self._launch_calls(frame, "dark", (3, 7))),
                         removal)
        self.assertEqual(self._harness_rules(self._live_calls(frame, "dark", (3, 2))), [])
        self.assertEqual(
            self._harness_rules(self._live_calls(_resolved(*["blue"] * 4),
                                                 "dark", (3, 7)))[0][-1],
            f"{commands_frame._CHROME_STYLE},bg=blue",
            "an agreeing arrangement got no surface either, so the removal above is not "
            "the disagreement's doing")

    def test_a_committed_colour_survives_the_level_that_used_to_erase_it(self):
        """`chrome: off` is a removal of the FRAME'S surface, never of a colour a
        component wrote. The rules stay the panels' colour, because the panels do — on the
        window below the floor, on the pane itself above it."""
        frame = _resolved(*["brightblack"] * 4)
        want = f"{commands_frame._CHROME_STYLE},bg=brightblack"
        self.assertEqual(self._live_value(frame, "off", (3, 2))[("-w", "pane-border-style")],
                         want)
        self.assertEqual(self._live_value(frame, "off", (3, 7))[("-p", "pane-border-style")],
                         want)

    def test_off_on_a_plane_that_named_no_colour_puts_the_rules_back(self):
        for v in (None, (3, 2), (3, 7)):
            with self.subTest(v=v):
                self.assertEqual(
                    self._live_value({"slots": ["top"]}, "off", v),
                    {("-w", n): commands_frame._CHROME_STYLE
                     for n in instance.PANE_BORDER_OPTIONS})

    def test_the_window_carries_the_surface_below_the_floor_and_the_pane_above_it(self):
        """The switch itself, on the live path: never both, and never neither. A window
        that kept the frame-wide surface above the floor would go on boxing the harness
        the per-pane edges exist to unbox; a pane written below it would land on the
        window anyway and the last panel would decide the frame."""
        frame = _resolved(*["brightblack"] * 4)
        want = f"{commands_frame._CHROME_STYLE},bg=brightblack"
        floor = self._live_value(frame, "dark", (3, 2))
        self.assertEqual(floor[("-w", "pane-border-style")], want)
        self.assertNotIn(("-p", "pane-border-style"), floor)
        above = self._live_value(frame, "dark", (3, 7))
        self.assertEqual(above[("-p", "pane-border-style")], want)
        self.assertEqual(above[("-w", "pane-border-style")],
                         commands_frame._CHROME_STYLE,
                         "the window kept a surface above the floor, so the rules around "
                         "the harness are still charter's colour")

    def test_a_frame_with_no_harness_pane_still_repaints_its_panes(self):
        """Skipped rather than refused: a frame launched by a charter that predates
        `record_harness_pane` has no record of one, and its panes are repainted above the
        rules. The harness pane is only the window SELECTOR here."""
        calls: list[list[str]] = []
        state.record_panes(self.FID, panels={"s0": "%2"})
        with mock.patch.object(config, "FRAME", _frame("brightblack")), \
                mock.patch.object(commands_frame.tmuxctl, "run",
                                  lambda _w, argv, **kw: calls.append(argv)), \
                mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID},
                                clear=True):
            self.assertEqual(
                commands_frame.cmd_chrome(type("A", (), {"level": "dark"})()), 0)
        self.assertEqual([a[-2] for a in calls if a[-2] in dict(commands_frame._CHROME)],
                         [])
        self.assertTrue([a for a in calls if "window-style" in a],
                        "the panes were not repainted either")

    def test_a_harness_pane_that_is_not_tmuxs_own_is_not_handed_to_tmux(self):
        """#475's rule about a value that arrived off DISK and is about to be a tmux argv,
        made here exactly as `cmd_chrome` already makes it of each panel's."""
        calls: list[list[str]] = []
        state.record_panes(self.FID, panels={"s0": "%2"})
        state.record_harness_pane(self.FID, "; kill-server")
        with mock.patch.object(config, "FRAME", _frame("brightblack")), \
                mock.patch.object(commands_frame.tmuxctl, "run",
                                  lambda _w, argv, **kw: calls.append(argv)), \
                mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID},
                                clear=True):
            commands_frame.cmd_chrome(type("A", (), {"level": "dark"})())
        for argv in calls:
            self.assertNotIn("; kill-server", argv)


if __name__ == "__main__":
    unittest.main()
