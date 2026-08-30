"""What tmux really does with the values `[frame] rules`, `[frame] text` and `[frame] dim`
produce — read off an attached client's wire, on this machine, at this moment.

**A pane border belongs to no pane, so `capture-pane` cannot see one**, and a pane's
DEFAULT foreground is not in the pane's content either — both are composed by tmux for the
client, at draw time. So the frame is rendered inside a second tmux: an OUTER server holds
one pane whose program is a client attached to the INNER server, which makes that pane's
content the frame's whole screen, borders and default cells included. `capture-pane -e -N`
on the outer pane then hands it back with the escapes still attached. That is
`test_frame_tmux_integration.py`'s `ChromeIsOneColour` harness, kept small here because
these tests need one panel rather than charter's whole shape.

**Every test renders the control first.** "The rule is invisible" is equally satisfied by a
machine that rendered no rule at all, and "the text is black" by a machine that rendered no
text — so each measurement is taken twice, once with the word and once without, and a
machine whose two readings agree SKIPS rather than passing quietly.

**And this file is where the third question is answered.** Charter cannot ship a coloured
frame as its default, because it cannot see the terminal (`[frame] chrome`'s own recorded
reasoning: OSC 11 through tmux answers nothing and `$COLORTERM` inside a pane describes the
terminal that started the SERVER). The obvious way out is an attribute rather than a
colour — `reverse` swaps whatever the operator's own two colours are, so it is right on
every theme, and tmux's own status line is reverse by default. `NoAttributeInAPaneStyle
ReachesTheWire` is the measurement that closes that door: tmux ACCEPTS every attribute in a
`window-style`, stores it, reads it back verbatim, and puts **nothing at all** on the wire
for it. Measured here on both 3.7c and at `tmuxctl.FLOOR`, which is the same answer
`frame/chrome.py`'s `recipes` recorded for a different phase and is now asserted rather
than remembered.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import time
import unittest
from pathlib import Path

from charter import commands_frame, instance
from charter.frame import tmuxctl
from tests import _tmuxreap
from tests._isolation import PersonaIso

_HAS_TMUX = shutil.which("tmux") is not None

#: The tmux this repo builds to measure its own floor against — `docs/frame.md` and
#: `test_frame_surface_live.py`'s module docstring both name it. Absent on CI, which
#: installs no tmux at all, so every floor test here skips there and says so.
_FLOOR_BIN = Path.home() / ".local/share/charter-testing" / f"tmux-{tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]}"

#: One SGR escape as it comes back out of `capture-pane -e`.
_SGR = re.compile(r"\x1b\[([0-9;]*)m")

#: A row of the box-drawing glyph tmux draws a horizontal rule with under
#: `pane-border-lines single` — the one charter pins.
_RULE_GLYPH = "─"


#: The SGR parameters that SET a foreground and a background to one of the sixteen — the
#: two ranges each, and the codes are ten apart by construction (ECMA-48 §8.3.117/118: 30-37
#: and 40-47, and the aixterm bright pair 90-97 and 100-107). That ten is what lets a test
#: say "the glyph is the colour it is sitting in" without naming a colour.
_FG_CODES = frozenset({*range(30, 38), *range(90, 98)})
_BG_CODES = frozenset({*range(40, 48), *range(100, 108)})

#: How far a background code sits above its own foreground code.
_FG_TO_BG = 10


def _colour_pair(params: list[str]) -> tuple[int | None, int | None]:
    """The foreground and background codes *params* leaves in effect, or ``None`` each.

    **Asked as a pair of codes rather than as a string, because the string is the machine's
    and the property is charter's.** Nesting one tmux inside another downsamples: at
    `tmuxctl.FLOOR` the outer client's own `default-terminal` renders this harness's
    `brightblack` as ``ESC[30m``/``ESC[40m`` rather than ``ESC[90m``/``ESC[100m``. Both
    readings say the same thing about charter — the glyph is the colour of the cell it sits
    in — and only one of them survives being spelled out, which is exactly the
    spelling-not-property mistake `frame/chrome.py` is written about.
    """
    fg = bg = None
    for group in params:
        for piece in group.split(";"):
            value = int(piece or "0")
            if value in _FG_CODES:
                fg = value
            elif value in _BG_CODES:
                bg = value
            elif value == 0:
                fg = bg = None
    return fg, bg


def _sgr_before(shot: str, needle: str) -> list[str]:
    """The SGR parameter lists in effect on the shot line that holds *needle*.

    Line-scoped, because `capture-pane -e` restates a line's attributes at its start: the
    escapes that matter for a cell are the ones on its own line, and reading across lines
    would attribute a neighbour's paint to it.
    """
    for line in shot.split("\n"):
        if needle in line:
            return [m.group(1) for m in _SGR.finditer(line.split(needle, 1)[0])]
    return []


class _NestedClient(PersonaIso):
    """An inner tmux showing a harness pane and one panel, seen through an outer one."""

    #: Overridden by the floor class.
    BIN = "tmux"

    def setUp(self) -> None:
        super().setUp()
        if not _HAS_TMUX:
            self.skipTest("no tmux on this machine, so there is no frame to look at")
        # **The binary is part of the socket name, and that is a defect this harness had
        # rather than belt-and-braces.** `TheFloorAnswersIdentically` subclasses the 3.7c
        # class and changes only `BIN`, so with a name built from the pid alone both would
        # reach for the same `/tmp/tmux-<uid>/<name>` — and a tmux 3.2 client cannot talk
        # to a 3.7c server that is still winding down from the previous test. Observed as
        # eight of fifteen failing on one run and none on the next, which is the signature
        # `#713` and `#719` both chased in the neighbouring harness. One socket per
        # binary per process, so the two versions never meet.
        tag = Path(self.BIN).name
        self._inner = _tmuxreap.name(f"frame-reads-in-{tag}")
        self._outer = _tmuxreap.name(f"frame-reads-out-{tag}")
        self.addCleanup(self._teardown)

    def _teardown(self) -> None:
        for socket in (self._outer, self._inner):
            self._tmux(socket, "kill-server")
            (Path("/tmp") / f"tmux-{os.getuid()}" / socket).unlink(missing_ok=True)

    def _tmux(self, socket: str, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run([self.BIN, "-L", socket, *args],
                              capture_output=True, text=True, timeout=20)

    def _shot(self, *, window_style: str | None, border_style: str | None,
              text: str = "PLAIN one") -> str:
        """Render one harness pane above one panel and return the outer pane's capture.

        *window_style* is set pane-scoped on the PANEL alone, exactly as `_surface_argvs`
        sets it — the harness pane is never an argument, which is ADR 0018's boundary held
        by construction here as it is in production.
        """
        # Counted, never hashed: `hash()` is salted per process and taking it modulo
        # anything can hand two different styles the same session name on one server.
        self._session_seq = getattr(self, "_session_seq", 0) + 1
        session = f"s{self._session_seq}"
        r = self._tmux(self._inner, "new-session", "-d", "-s", session,
                       "-x", "60", "-y", "12", "--", "cat")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness = self._tmux(self._inner, "list-panes", "-t", session,
                             "-F", "#{pane_id}").stdout.strip()
        self._tmux(self._inner, "set", "-t", session, "status", "off")
        # `pane-border-lines single` is charter's own pin and the reason the glyph below is
        # the one this file looks for. `pane-border-indicators` does not exist at the
        # floor, so it is set where it exists and skipped where it does not — production
        # issues it either way and `tmuxctl.run` reports the failure without refusing.
        for name, value in (("pane-border-lines", "single"),
                            ("pane-border-status", "off")):
            self._tmux(self._inner, "set", "-w", "-t", harness, name, value)
        if border_style is not None:
            for name in instance.PANE_BORDER_OPTIONS:
                self.assertEqual(
                    self._tmux(self._inner, "set", "-w", "-t", harness, name,
                               border_style).returncode, 0, border_style)
        panel = self._tmux(self._inner, "split-window", "-t", harness, "-v",
                           "-P", "-F", "#{pane_id}", "-l", "4", "--",
                           "sh", "-c", f'printf "{text}\\r\\n"; cat').stdout.strip()
        if window_style is not None:
            for name in instance.chrome_option_names():
                self.assertEqual(
                    self._tmux(self._inner, "set", "-p", "-t", panel, name,
                               window_style).returncode, 0, window_style)
        self._tmux(self._inner, "select-pane", "-t", harness)
        host = f"h{session}"
        self.assertEqual(
            self._tmux(self._outer, "new-session", "-d", "-s", host, "-x", "60", "-y", "12",
                       "--", self.BIN, "-L", self._inner, "attach", "-t", session
                       ).returncode, 0)
        self._tmux(self._outer, "set", "-t", host, "status", "off")
        shot = ""
        deadline = time.time() + 15
        while time.time() < deadline:
            # `-N` keeps the trailing spaces, and without it a painted pane is invisible to
            # this harness: the panel writes one line and tmux fills the rest with spaces,
            # which `capture-pane` trims — taking the paint's own SGR with them.
            got = self._tmux(self._outer, "capture-pane", "-p", "-e", "-N", "-t", host)
            if got.returncode == 0:
                shot = got.stdout
            if text in shot and _RULE_GLYPH in shot:
                break
            time.sleep(0.1)
        self._tmux(self._outer, "kill-session", "-t", host)
        self._tmux(self._inner, "kill-session", "-t", session)
        if _RULE_GLYPH not in shot:
            self.skipTest("this machine rendered no pane border through a nested client, "
                          "so there is nothing here to measure the colour of")
        return shot


class TheHiddenRuleIsDrawnAndCannotBeSeen(_NestedClient, unittest.TestCase):
    """`[frame] rules = "hidden"` — the shipped default, rendered.

    tmux always paints a glyph on a border cell, so the seam the operator reported four
    times cannot be removed by asking for fewer characters. It is removed by giving the
    glyph the colour it is sitting in — which is exactly the style they hand-applied to
    their own live frame and confirmed, `fg=brightblack,bg=brightblack`, and which
    `instance.rule_style` now composes from the word.
    """

    #: The surface the operator's own plane wears on every panel.
    SURFACE = "bg=brightblack"

    def _rule_sgr(self, border_style: str) -> list[str]:
        shot = self._shot(window_style=self.SURFACE, border_style=border_style)
        return _sgr_before(shot, _RULE_GLYPH)

    def test_the_seam_the_operator_reported_really_does_render(self):
        """The control, and the reason it is not decoration: without it "the fixed rule is
        the surface's colour" is satisfied by a machine that rendered no colour at all."""
        params = self._rule_sgr(instance.rule_style(
            self.SURFACE, commands_frame._CHROME_FG,
            instance.Look("visible", "default", True)))
        self.assertIn("2", params,
                      f"the dim that IS the seam never reached the wire: {params}")

    def test_the_shipped_rule_puts_the_surface_on_the_glyph_as_well(self):
        """`ESC[90m` on `ESC[100m`: `brightblack` foreground on `brightblack` background,
        so tmux draws the glyph and there is nothing to see. Asserted as the two SGR
        parameters rather than as a string, because what an operator looks at is the
        colour pair and not charter's spelling of it."""
        params = self._rule_sgr(instance.rule_style(
            self.SURFACE, commands_frame._CHROME_FG, instance.SHIPPED_LOOK))
        fg, bg = _colour_pair(params)
        self.assertIsNotNone(fg, f"the glyph did not take a colour at all: {params}")
        self.assertIsNotNone(bg, f"the rule lost its background: {params}")
        self.assertEqual(bg, fg + _FG_TO_BG,
                         f"the glyph is not the colour it is sitting in: {params}")
        self.assertNotIn("2", params, f"an attribute survived `hidden`: {params}")

    def test_the_two_renderings_really_are_different(self):
        """The control and the fix, side by side — a machine whose readings agree has
        measured nothing, and says so rather than passing."""
        visible = self._rule_sgr(instance.rule_style(
            self.SURFACE, commands_frame._CHROME_FG,
            instance.Look("visible", "default", True)))
        hidden = self._rule_sgr(instance.rule_style(
            self.SURFACE, commands_frame._CHROME_FG, instance.SHIPPED_LOOK))
        if visible == hidden:
            self.skipTest(f"this tmux renders both rules identically ({visible}), so "
                          "there is no difference here to attribute to the word")
        self.assertNotEqual(visible, hidden)


class TheFrameSTextIsThePanesOwnDefault(_NestedClient, unittest.TestCase):
    """`[frame] text` — one key, every cell, and no renderer told anything.

    The whole design rests on one property of tmux: a `window-style` carries an `fg` as
    well as a `bg`, and tmux resolves the pane's DEFAULT foreground from it. If that were
    false, `text` would have to be a substitution over forty `statusline._DIM` call sites
    in `frame/slots.py` plus whatever a provider's component writes. It is true, on 3.7c
    and at the floor, which is what these read off the wire.
    """

    def test_a_pane_with_only_a_background_leaves_the_foreground_the_terminals(self):
        """The control: with no `fg` in the style, nothing on the panel's line names a
        foreground colour, so the operator's own is what draws it."""
        fg, bg = _colour_pair(_sgr_before(self._shot(window_style="bg=brightblack",
                                                     border_style=None), "PLAIN one"))
        self.assertIsNotNone(bg, "the pane was not painted at all")
        self.assertIsNone(fg, f"a foreground appeared from nowhere: {fg}")

    def test_a_pane_that_names_a_foreground_draws_its_text_in_it(self):
        """`ESC[30m` beside the `ESC[100m`: the plane's word, on text no renderer coloured.
        This is what makes one frame-wide key reach every uncoloured cell in the frame."""
        style = dict(instance.surface_options(
            "brightblack", "off", instance.Look("hidden", "black", True)))["window-style"]
        fg, bg = _colour_pair(_sgr_before(self._shot(window_style=style,
                                                     border_style=None), "PLAIN one"))
        self.assertIsNotNone(fg, "the plane's foreground never reached a cell")
        self.assertIsNotNone(bg, "the background went with it")
        self.assertNotEqual(bg, fg + _FG_TO_BG,
                            "the text came out the colour of its own background")

    def test_charters_own_reset_returns_to_the_planes_colour_and_not_the_terminals(self):
        """The half that makes it work without touching a renderer. Charter writes
        `statusline._R` — a full `ESC[0m` — after every coloured span, and if that returned
        to the TERMINAL's foreground the plane's word would apply only to the cells nobody
        wrote. It does not: tmux re-states the pane's own default after the reset."""
        style = dict(instance.surface_options(
            "brightblack", "off", instance.Look("hidden", "black", True)))["window-style"]
        shot = self._shot(window_style=style, border_style=None,
                          text="\\033[32mGREEN\\033[0m AFTER")
        params = _sgr_before(shot, "AFTER")
        self.assertIsNotNone(
            _colour_pair(params)[0],
            f"a reset inside the pane went back to the terminal's own foreground, so the "
            f"plane's word would cover only the cells no renderer wrote: {params}")


class NoAttributeInAPaneStyleReachesTheWire(_NestedClient, unittest.TestCase):
    """**The third question, answered by measurement: `reverse` cannot be the default.**

    A default surface has to be theme-independent, because charter cannot see the theme.
    `reverse` is the one candidate that is — it swaps whatever the operator's own two
    colours are, so a reversed panel is distinct from the work area on every theme, and
    tmux's own status line has used it forever. It does not work, and the failure is not
    subtle: tmux accepts the value, stores it, reads it back verbatim, and emits **nothing
    at all**.

    So the questions that would have followed it do not arise. It cannot leave twelve rows
    of a fifteen-row pane unreversed, because it reverses none of them. It cannot compose
    badly with `chrome.recipes()`' green, because there is nothing for the green to compose
    with. It cannot conflict with the `rules` work, because there is no rendering to
    conflict.

    **And a renderer-side reverse is not the way round it.** `_surface_argvs`' own docstring
    measures why: tmux fills a pane's whole rectangle from `window-style` — the cells no
    renderer wrote included, on resize, on reattach — and a fill a panel painted itself is a
    property of the PROCESS rather than of the rectangle. A panel that reversed its own rows
    would leave every row it did not write, and every row a resize added, in the terminal's
    own colours until the next tick.

    The consequence is stated in `docs/frame.md` rather than worked around: there is no
    theme-independent surface, `chrome` stays `off`, and a plane that wants one writes three
    lines.
    """

    def test_tmux_stores_an_attribute_in_a_pane_style_and_reads_it_back(self):
        """The control for the measurement below: an attribute is not REFUSED, which is
        what makes the silence worth a test. A charter that set `window-style reverse` and
        checked the return code would report success on every tmux and paint nothing."""
        r = self._tmux(self._inner, "new-session", "-d", "-s", "probe", "--", "cat")
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = self._tmux(self._inner, "list-panes", "-t", "probe",
                          "-F", "#{pane_id}").stdout.strip()
        for value in ("reverse", "bold", "dim", "fg=default,bg=default,reverse"):
            with self.subTest(style=value):
                self.assertEqual(
                    self._tmux(self._inner, "set", "-p", "-t", pane,
                               "window-style", value).returncode, 0)
                self.assertEqual(
                    self._tmux(self._inner, "show", "-pv", "-t", pane,
                               "window-style").stdout.strip(), value)
        self._tmux(self._inner, "kill-session", "-t", "probe")

    def test_a_colour_in_the_same_option_really_does_reach_the_wire(self):
        """The other control, and the one that makes the silence attributable to the
        ATTRIBUTE rather than to this harness: the same option, the same pane, a colour."""
        params = _sgr_before(self._shot(window_style="bg=brightblack",
                                        border_style=None), "PLAIN one")
        self.assertIsNotNone(_colour_pair(params)[1],
                             f"this harness cannot see a pane style at all: {params}")

    def test_no_attribute_puts_a_single_escape_on_an_attached_clients_wire(self):
        """`reverse` (SGR 7), `bold` (1) and `dim` (2), each set on both style options of a
        real pane and read off a real client. None of them appears."""
        for value, sgr in (("reverse", "7"), ("bold", "1"), ("dim", "2")):
            with self.subTest(style=value):
                params = _sgr_before(self._shot(window_style=value, border_style=None),
                                     "PLAIN one")
                self.assertNotIn(sgr, params,
                                 f"`window-style {value}` reached the wire after all — "
                                 f"the recorded measurement is wrong and a "
                                 f"theme-independent default surface may be possible: "
                                 f"{params}")


@unittest.skipUnless(_FLOOR_BIN.exists(),
                     f"no {_FLOOR_BIN.name} built on this machine — `docs/frame.md` says "
                     "how, and CI installs no tmux at all")
class TheFloorAnswersIdentically(TheHiddenRuleIsDrawnAndCannotBeSeen):
    """Every rendering above, re-run against `tmuxctl.FLOOR`.

    The floor is the version that decides whether any of this needs a gate. It did not for
    `window-style` (`test_frame_surface_live.py`'s Phase 3.5), and it does not here: a
    `pane-border-style` carrying `fg=` and `bg=` is one value on one option, which 3.2 has
    had since long before charter existed. Measured rather than inherited — the class it
    subclasses is the measurement, and only the binary changes.

    **Subclassed deliberately, which is the one place this suite's own "a mixin, never a
    base TestCase" rule does not apply**: re-running these three renderings on a second
    tmux is exactly what this class is for, so inheriting them is the feature rather than
    the surprise `_TmuxServerFixture`'s docstring warns about.
    """

    BIN = str(_FLOOR_BIN)


@unittest.skipUnless(_FLOOR_BIN.exists(), "no floor tmux built on this machine")
class TheFloorIsSilentAboutAttributesToo(NoAttributeInAPaneStyleReachesTheWire):
    """The `reverse` finding at the floor. If the two versions disagreed, a default could
    in principle be gated at the version that works — the way `display-popup` is gated at
    3.3. They do not disagree, so there is nothing to gate."""

    BIN = str(_FLOOR_BIN)
