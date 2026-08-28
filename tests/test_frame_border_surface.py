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
from charter.frame import state
from tests._isolation import PersonaIso

_STYLES = ("pane-border-style", "pane-active-border-style")


def _frame(*bgs, **rest) -> dict:
    """A resolved `[frame]` mapping whose arrangement names one component per *bgs*
    entry — `None` for a component that names no background of its own."""
    return {"components": [{"slot": f"s{i}", "use": f"s{i}",
                            **({} if bg is None else {"bg": bg})}
                           for i, bg in enumerate(bgs)], **rest}


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

    def _issued(self, frame: dict, slots: list[str], *, fid="f-1") -> list[list[str]]:
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
                                         harness_pane="%1", env=None, pane_env=None)
        return seen

    def _rules(self, issued: list[list[str]]) -> dict[str, str]:
        return {a[-2]: a[-1] for a in issued
                if "set-option" in a and "-w" in a and a[-2] in dict(commands_frame._CHROME)}

    def _frame(self, **style) -> dict:
        tables = [{"use": cid, **style.get(cid, {})}
                  for cid in ("identity", "attention", "repos", "sidebar")]
        return instance.frame_of({"frame": {"component": tables}, })

    def test_a_launch_draws_its_rules_in_the_colour_its_panels_agree_on(self):
        frame = {**self._frame(**{c: {"bg": "brightblack"} for c in
                                  ("identity", "attention", "repos", "sidebar")}),
                 "chrome": "dark"}
        rules = self._rules(self._issued(frame, ["top", "bottom", "repos", "right"]))
        self.assertEqual(rules["pane-border-style"],
                         f"{commands_frame._CHROME_STYLE},bg=brightblack")
        self.assertEqual(rules["pane-active-border-style"], rules["pane-border-style"])

    def test_a_launch_that_names_no_colour_arms_exactly_what_it_always_did(self):
        self.assertEqual(self._rules(self._issued({}, ["top", "bottom"])),
                         dict(commands_frame._CHROME))

    def test_a_relayout_that_adds_one_pane_still_arms_the_WHOLE_frames_surface(self):
        """`_relayout` calls this funnel with only the slots being ADDED, so a rule colour
        derived from the *slots* argument would change every time a density level brought
        a panel back — and would be wrong for the panels already on screen. It is derived
        from `config.FRAME` instead, which is the same thing `cmd_chrome` reads."""
        frame = {**self._frame(**{c: {"bg": "blue"} for c in
                                  ("identity", "attention", "repos", "sidebar")}),
                 "chrome": "dark"}
        whole = self._rules(self._issued(frame, ["top", "bottom", "repos", "right"]))
        one = self._rules(self._issued(frame, ["right"]))
        self.assertEqual(one, whole)
        self.assertEqual(one["pane-border-style"],
                         f"{commands_frame._CHROME_STYLE},bg=blue")

    def test_the_live_word_and_not_the_committed_one_decides_the_rules(self):
        """`_split_panels` resolves the level through `_current_chrome`, so a pane split
        into a frame the operator has since surfaced from the palette is bordered with the
        surface the frame IS rather than the one it launched with — the same resolver the
        pane's own background already goes through."""
        frame = {**self._frame(), "chrome": "off"}
        state.record_chrome("f-live", "dark")
        rules = self._rules(self._issued(frame, ["top"], fid="f-live"))
        self.assertEqual(rules["pane-border-style"],
                         f"{commands_frame._CHROME_STYLE},"
                         f"{dict(instance.FRAME_CHROME['dark'])['window-style']}")


class TheLaunchPathAndTheLivePathAgree(PersonaIso, unittest.TestCase):
    """#610's pin, on the border: what a frame LAUNCHES with and what a keypress leaves it
    with must be the same value, at every level and for every colour.

    The defect that rule exists for is specific and was paid for once already: a component
    with `bg = "brightblack"` repainted in the frame-wide colour by `chrome: light`, and
    ERASED by `chrome: off`, with nothing to bring it back until relaunch.
    """

    FID = "fr-border"

    def _launch_value(self, frame, level):
        with mock.patch.object(config, "FRAME", frame), \
                mock.patch.object(commands_frame, "_current_chrome",
                                  lambda _fid: level):
            chrome = commands_frame._current_chrome(self.FID)
            return {a[-2]: a[-1] for a in commands_frame._chrome_argvs(
                socket="s", harness_pane="%1",
                surface=instance.border_bg(config.FRAME, chrome))}

    def _live_value(self, frame, level):
        calls: list[list[str]] = []
        state.record_panes(self.FID, panels={"s0": "%2"})
        state.record_harness_pane(self.FID, "%1")
        with mock.patch.object(config, "FRAME", frame), \
                mock.patch.object(commands_frame.tmuxctl, "run",
                                  lambda _w, argv, **kw: calls.append(argv)), \
                mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID},
                                clear=True):
            commands_frame.cmd_chrome(type("A", (), {"level": level})())
        return {a[-2]: a[-1] for a in calls if a[-2] in dict(commands_frame._CHROME)}

    def test_every_level_crossed_with_every_colour_lands_on_one_value(self):
        for level in instance.FRAME_CHROME:
            for word in (None, *instance.FRAME_PANE_BG):
                frame = _frame(word, word)
                with self.subTest(level=level, bg=word):
                    self.assertEqual(self._launch_value(frame, level),
                                     self._live_value(frame, level))

    def test_a_committed_colour_survives_the_level_that_used_to_erase_it(self):
        """`chrome: off` is a removal of the FRAME'S surface, never of a colour a
        component wrote. The rules stay the panels' colour, because the panels do."""
        frame = _frame(*["brightblack"] * 4)
        self.assertEqual(
            self._live_value(frame, "off")["pane-border-style"],
            f"{commands_frame._CHROME_STYLE},bg=brightblack")

    def test_off_on_a_plane_that_named_no_colour_puts_the_rules_back(self):
        self.assertEqual(self._live_value({}, "off"), dict(commands_frame._CHROME))

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
