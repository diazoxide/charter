"""The frame's paint: filling a row to the pane's edge, and highlighting one.

**Everything here takes a FINISHED row** — a string some `tui` node has already
composed and clamped — and hands back a string that must not go back through `tui`.
That ordering is the whole reason this module exists rather than a `tui` node.
`tui._finish` strips trailing whitespace, *including* whitespace hiding behind trailing
SGR escapes, and it is right to: `tui` also draws the status line, which writes into a
line it does not own and must never leave painted cells trailing across somebody else's
prompt. Measured on this machine, a 20-cell fill in and an 8-cell line out, both ways
round::

    fill inside the span  in ='\\x1b[48;5;236m charter            \\x1b[0m'   width 20
                          out='\\x1b[48;5;236m charter\\x1b[0m'               width  8
    pad outside the span  in ='\\x1b[48;5;236m charter\\x1b[0m            '   width 20
                          out='\\x1b[48;5;236m charter\\x1b[0m'               width  8

So a caller who reverses the order — fills first, composes second — gets a row that
silently comes back at its natural width with the paint gone, and nothing raises.
Compose through `tui` as today, **then** hand the finished string here.

**Charter does not paint the pane's background.** tmux does, per pane, from
`window-style`/`window-active-style` (`instance.FRAME_CHROME`), which is why nothing
in this module is on the repaint path for a *surface*. What is here is the one element
that cannot be a pane option: a single row picked out of a list.

**Nothing here names a colour.** Reverse video is defined as the operator's own
foreground and background exchanged, so it is correct on every theme — including the
Solarized palettes, where every named grey charter could have chosen *is* somebody's
background — and it survives every terminal tier tmux downsamples for: `tui.sanitize`
keeps SGR 7, tmux passes `ESC[7m` through to `xterm` and `vt100` alike, and on a client
with no colour at all tmux converts its own colour *to* reverse.
"""

from __future__ import annotations

import os
import re
from types import MappingProxyType

from .. import tui
from . import pane

#: Reverse video on, and everything off. `_OFF` is a FULL reset rather than SGR 27
#: ("reverse off"): a row that left an attribute open — a provider's, or a charter row
#: after a future edit — would otherwise carry it past the end of the highlight into the
#: rest of the pane. Reverse is one of the attributes a full reset clears, so this is the
#: stronger form of the same statement rather than a different one.
_REVERSE = "\x1b[7m"
_OFF = tui.RESET

#: One SGR escape, with its parameter list captured. Deliberately the same shape
#: `tui._SGR` matches, because :func:`tui.sanitize` has already deleted everything else
#: that claims to be an escape by the time a row reaches this module.
_SGR = re.compile(r"\x1b\[([0-9;]*)m")


def resets_everything(params: str) -> bool:
    """True when the SGR parameter list *params* turns every attribute off.

    **The property is the NUMBER, not the spelling**, and this function exists because
    the first version of :func:`reverse` got that wrong in the way this project has now
    got it wrong six times (#547, #558, #537, #498, #577, #594). "An SGR that resets
    everything" is not the string ``\\x1b[0m``. It is a parameter list containing a
    parameter whose *numeric value* is zero — and an **empty** parameter is zero (ECMA-48
    §5.4.2: an omitted parameter takes its default, and SGR's default is 0), and leading
    zeros are legal digits. Measured, the six spellings that matter::

        spelling      params    string-match ("" or "0")   numeric value == 0
        '\\x1b[0m'     '0'       True                       True
        '\\x1b[m'      ''        True                       True
        '\\x1b[00m'    '00'      False   <- MISSED          True
        '\\x1b[1;00m'  '1;00'    False   <- MISSED          True
        '\\x1b[2m'     '2'       False                      False
        '\\x1b[22;39m' '22;39'   False                      False

    A row carrying ``\\x1b[00m`` would highlight for half its width and every test
    written against ``\\x1b[0m`` would pass. Charter writes ``\\x1b[0m`` today
    (`statusline._R`), so the string test would be right about charter's own rows and
    wrong about a provider's — and what a provider writes is not charter's to choose.

    ANY zero in the list, not only a leading one: parameters apply left to right, so
    ``1;0`` is bold-then-reset and ``0;1`` is reset-then-bold. Reverse is off after
    either, which is the question this answers.
    """
    # Every piece is digits or empty — `_SGR` admits nothing else — so `int` cannot
    # raise here, and an empty piece is spelled as the zero it means.
    return any(int(p or "0") == 0 for p in params.split(";"))


#: SGR parameters that turn reverse video off: 0 (everything off) and 27 (reverse off).
#: Named as a set so :func:`cancels_reverse` reads as the question it asks.
_REVERSE_OFF = frozenset({0, 27})


def cancels_reverse(params: str) -> bool:
    """True when the SGR parameter list *params* leaves reverse video OFF.

    :func:`resets_everything`'s question asked about one attribute — the same numeric
    reading, and **one parameter more**: SGR 27 turns reverse off on its own without
    touching anything else. Charter writes no `\\x1b[27m` today, but a provider's
    component is ordinary Python writing into a row charter is about to highlight, and a
    highlight that survived `\\x1b[0m` and died at `\\x1b[27m` would be the same defect
    with a different spelling.

    Not folded into :func:`resets_everything`, because the two are asked by different
    callers about different things: `_split_at_close` wants to know whether a trailing
    escape leaves any span open, and `\\x1b[27m` leaves a colour open.
    """
    return any(int(p or "0") in _REVERSE_OFF for p in params.split(";"))


def no_colour() -> bool:
    """Whether the operator has asked for no colour at all.

    **One question asked in one place.** Two implementations of "did they set
    ``NO_COLOR``" is how the two come to disagree about ``NO_COLOR=""``, which is #547's
    shape and the reason this is a function rather than a check spelled at each call
    site — here, in :func:`colour_ok`, and in `commands_frame._surface_argvs`, which is a
    different process on a different path asking the same thing.

    Honoured by **presence**, per the `no-color.org` convention: any value, including the
    empty string and ``0``, means no colour. Matching a *value* (``== "1"``) would be the
    spelling-not-property mistake in the one file that is about that mistake — and it is
    the reading that breaks first, because a shell that exports ``NO_COLOR=`` has set it.

    **It reaches the tmux surface too**, which is the half that is easy to miss. The pane
    background is painted by tmux rather than by charter, so gating only charter's own
    SGR would leave an operator who asked for no colour looking at a coloured frame —
    charter having asked somebody else to paint it. That is honouring the letter of the
    promise and not the property.
    """
    return os.environ.get("NO_COLOR") is not None


def colour_ok() -> bool:
    """Whether the frame may emit SGR into this pane.

    :func:`no_colour` and one more question. ``isatty`` does almost nothing in a real
    frame and is here for completeness rather than as the mechanism: a panel's stdout
    *is* its pane, so it is always a tty in production. The one case it catches is the
    redirect `panel.py`'s own docstring documents — ``charter panel top --session x >
    /tmp/log`` — which before this wrote a clear-screen and full SGR into a file.

    **Asked of the PANE, not of `sys.stdout`** (#606). Those were the same thing until a
    provider's library rebound the global: Textual's `redirect_stdout` installs a
    `_PrintCapture` that answers ``isatty() -> True`` from behind fd -1, so this said
    "colour is fine" about a stream that is not a terminal and, more to the point, is not
    the rectangle charter is painting. `frame/pane.py` holds the descriptor this process
    was actually given and carries the "a stream that cannot say is not a terminal"
    fallback this used to spell here — one answer, so the paint and the question about the
    paint cannot be about two different streams.
    """
    if no_colour():
        return False
    return pane.is_tty()


def _role_values() -> dict[str, str]:
    """The roles of §5 a RENDERER can write, and the SGR each one is. **The vocabulary.**

    One source, not a list of names beside a table of values: two of those is how a role
    comes to be documented and unserved, or served and undocumented, and the caller below
    iterates whatever this returns.

    **This is deliberately not the same list as §5's six.** Two of those — ``surface`` and
    ``focus`` — are `window-style`/`window-active-style`, tmux pane options that no
    renderer can emit and no provider can be handed. Serving them here as empty strings
    would be a recipe that claims to hand over something charter does not have, which is
    the same convincing empty `instance.FRAME_CHROME` refuses an ``auto`` value for. A
    provider that wants the surface already has it: tmux painted the rectangle underneath
    before the provider drew a cell.

    **Every value is an attribute or one of the sixteen ANSI names — no cube index, no
    24-bit triple.** That is `instance.FRAME_CHROME`'s rule reaching the one place a
    stranger's code would otherwise have to guess it, and it is why these need no
    `[frame] chrome` gate: bold, dim and reverse are statements relative to whatever the
    operator's terminal already is, and ``green``/``yellow``/``red`` are slots in their
    own palette rather than colours charter picked out of the 256.

    Composed from `statusline.py`'s OWN constants rather than spelled here, for
    `slots._sidebar_head`'s reason: a second copy of ``\033[1m`` in this module is how a
    provider's heading and charter's own come to be two different weights. A
    function-level import for that function's other reason — `statusline` is the largest
    module charter has and a panel repaints on a clock.
    """
    from .. import statusline as sl
    return {"heading": sl._BOLD, "muted": sl._DIM, "selected": _REVERSE,
            "ok": sl._GREEN, "warn": sl._YELLOW, "bad": sl._RED, "reset": sl._R}


def recipes() -> MappingProxyType:
    """The role → string mapping a component is handed: `ctx.chrome`.

    **What a provider gets so it can match without charter overdrawing it** (§7). Charter
    owns the surface and the border; the provider owns every cell it writes. A component
    that writes ``ctx.chrome["heading"]`` puts its label in the same weight charter's own
    sidebar heading uses; one that does not looks different, which is honest, because it
    *is* different — and a frame where every pane looked identical regardless of who wrote
    it would hide the one thing an operator needs to know when a pane is wrong.

    **A `MappingProxyType` of strings, no callable, reading nothing.** The same shape
    `ctx.SERVES["gather"]` already hands over, and the reason is `ctx.Ctx`'s own: a
    provider's module is ordinary Python, so what must not be reachable from this object
    is anything that *does* something. A plain `dict` would be worse than merely mutable —
    one snapshot is shared by every component in a repaint, so a component that edited it
    would be editing what the next one is about to draw.

    **`inset` is the one entry that is not SGR, and it is here on purpose.** §5.4 is a
    COLUMN rather than an attribute — the value `slots.INSET` already holds — and a
    provider that wants its rows to start where charter's do needs the string, not the
    number. Served as the literal left edge (`slots._inset()`) so a provider prepends it
    and lines up, rather than being told a count and left to spell the padding itself,
    which is the per-call-site spelling §5.4 exists to end. It is NOT suppressed by
    :func:`colour_ok` below: two spaces are not colour, and a frame that lost its inset
    under ``NO_COLOR`` would be answering a question about colour with a change to layout.

    **`chrome` — the WORD — changes none of these, and that is a measurement rather than
    an omission.** §7 asks for the roles "resolved for this frame's `chrome` setting", and
    when this was built there was nothing for that setting to resolve: `off`, `dark` and
    `light` select a pane background, `window-style` honours colour and silently ignores
    every attribute (measured again for this phase on tmux 3.7c AND on tmux 3.2 — `bold`,
    `dim` and `reverse` each put no SGR at all on an attached client's wire), and no
    renderer can write a pane option. So a `chrome` parameter here would be an argument
    that cannot change an answer, which is the line this repo's own sweep deletes. What
    IS resolved is :func:`colour_ok`, which is per pane and does change every value.

    **Under `NO_COLOR`, or a stdout that is not a tty, every SGR role is the empty
    string.** §3.2's rule is that charter emits no SGR from the frame at all, and a
    provider handed live escapes there would emit them on charter's behalf — charter
    having asked somebody else to paint again, which is the half of that promise this
    spec was written about. Empty rather than absent: a provider that wrote
    ``ctx.chrome["ok"] + text`` would otherwise raise inside its own draw and lose its
    pane to honour a request about colour.
    """
    from . import slots
    live = colour_ok()
    out = {role: (sgr if live else "") for role, sgr in _role_values().items()}
    out["inset"] = slots._inset()
    return MappingProxyType(out)


def plain(row: str) -> str:
    """*row* as the text a terminal with no colour would show — every SGR removed.

    Trailing whitespace goes with it, which is :func:`fill`'s pad: with nothing painted
    there is nothing for it to paint, and a row of invisible spaces is only something to
    copy out of the pane by accident.
    """
    return tui.strip_ansi(row).rstrip()


def _split_at_close(row: str) -> tuple[str, str]:
    """*row* split where a pad belongs: before a trailing run that only CLOSES a span.

    The pad has to land where the row's style is the style the pad should carry, and
    that position is not "the end" and not "before the last escape" — it depends on what
    the trailing escapes do:

    * ``'\\x1b[48;5;236m charter\\x1b[0m'`` ends by closing its span, so the pad goes
      **inside** it — before the reset — or the fill stops at the text.
    * ``'…\\x1b[0m\\x1b[7m'`` (what :func:`reverse` composes) ends with a span still
      OPEN, so the pad goes **after** it and is painted.

    So the split is at the start of the trailing SGR run **only when every escape in it
    resets everything**. Asked as a property of the parameters (:func:`resets_everything`)
    rather than by looking for ``\\x1b[0m``.
    """
    start = len(row)
    for m in reversed(list(_SGR.finditer(row))):
        if m.end() != start:        # the trailing run has ended: text below it
            break
        if not resets_everything(m.group(1)):
            return row, ""          # a span is still open — the pad belongs after it
        start = m.start()
    return row[:start], row[start:]


def fill(row: str, width: int) -> str:
    """*row* padded to exactly *width* visible cells, the pad inside its style span.

    Takes a **finished** row (see the module docstring) and returns one that must not be
    handed back to `tui`.

    **Exactly *width*, never more, and the clamp is not decoration.** Measured in a real
    20-column tmux pane, one row each at W−1, W and W+1 cells::

        row 0: '\\x1b[48;5;196mAAAAAAAAAAAAAAAAAAA\\x1b[49m '   <- 19 cells: bg stops short
        row 1: '\\x1b[48;5;46mBBBBBBBBBBBBBBBBBBBB'            <- 20 cells, exactly W: SAFE
        row 2: '\\x1b[48;5;21mCCCCCCCCCCCCCCCCCCCC'            <- 21 cells: wraps…
        row 3: 'C\\x1b[49m    '                                <- …one cell onto the next
        row 4: '\\x1b[48;5;226mDDD\\x1b[49m  '                  <- every row below shifted

    Exactly W is safe — the deferred-wrap state is resolved by the following newline and
    produces no blank row. **W+1 shears the pane**, which is #553 arriving through a new
    door. Today's rows are ragged and short, so an off-by-one is a cosmetic gap; a row
    painted to the edge is one cell from shearing. `tui.truncate` is what enforces it, so
    an over-wide row comes back cut with an ellipsis rather than wrapped — the same
    answer every other row in the frame already gets, and a visible one.

    *width* must be the width the RENDERER measured (`slots._width()`, the pane's own
    tty). A panel inherits the launching shell's `$COLUMNS` whole — measured, a
    22-column pane whose launcher had exported `COLUMNS=200` — so a fill computed at 200
    in a 22-column pane wraps every row four times over. One measurement, not two.

    **A pane with no columns at all gets nothing, and that is `tui.truncate`'s refusal
    rather than a second one here.** An `if width <= 0: return ""` above this line was
    written first and then deleted: `truncate` already answers `""` for a non-positive
    width, so the pad below is never reached and the guard could not change an outcome.
    A line that cannot is not documentation of an intent — it is a second, weaker answer
    to a question that was already answered, which is why #568 deleted the last one of
    those. `reverse` keeps its own, because there it is live.
    """
    row = tui.truncate(row, width)
    # `" " * n` is "" for every n <= 0 and `_split_at_close` partitions the row, so a row
    # that already fills the pane comes back out of this line unchanged. An
    # `if gap <= 0: return row` above it was written first and deleted: the deletion
    # sweep found it surviving, correctly — it could not change an outcome.
    head, close = _split_at_close(row)
    return head + " " * (width - tui.width(row)) + close


def reverse(row: str, width: int) -> str:
    """*row* highlighted to the pane's last column: full-width reverse video.

    **A highlight cannot be a wrapper around an already-composed row**, and that is the
    defect this function exists to answer. Charter's rows carry `statusline._R` after
    every coloured span, and a full reset cancels *reverse* along with everything else.
    Measured, the real sidebar row wrapped naively::

        row : '\\x1b[35m▸ \\x1b[1msteward\\x1b[0m   \\x1b[32m✎47\\x1b[0m'
        out : '\\x1b[7m\\x1b[35m▸ \\x1b[1msteward\\x1b[0m   \\x1b[32m✎47\\x1b[0m<pad>\\x1b[27m'
                                                ^^^^^^^^ reverse ends here, 22 chars in

    The row is highlighted for two words and plain for the rest. So reverse is
    **re-asserted after every SGR that leaves it off** — and what counts as one is
    :func:`cancels_reverse`'s numeric question, not a search for ``\\x1b[0m``. Its
    parameter 27 half is not hypothetical tidiness: a provider's component writes into a
    row charter is about to highlight, and a highlight that survived ``\\x1b[0m`` and
    died at ``\\x1b[27m`` would be the same defect wearing a different number.

    **Only where it was cancelled, and not after every escape.** Re-asserting
    unconditionally keeps the row highlighted just as well and says so three times as
    often — bytes written into a pane on every repaint that change nothing on the screen.

    Applied to the row a renderer has already finished, at the width that renderer
    measured. It needs no `[frame] chrome` gate: reverse names no colour, so there is no
    theme it can be wrong on.

    **The no-columns refusal is live here and it is not in `fill`.** Without it a pane of
    zero width still gets `_OFF` written into it — a bare `\\x1b[0m` where the caller
    asked for nothing at all. `fill` needs no such line because `tui.truncate` answers
    the same question one call earlier; `reverse` appends after that answer, so it has to
    ask for itself.
    """
    if width <= 0:
        return ""
    body = _SGR.sub(
        lambda m: m.group(0) + (_REVERSE if cancels_reverse(m.group(1)) else ""),
        tui.truncate(row, width))
    return fill(_REVERSE + body, width) + _OFF
