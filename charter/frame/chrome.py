"""The frame's paint: filling a row to the pane's edge, highlighting one, and deciding
what a stranger's row may keep of it.

**The vocabulary and the containment of that vocabulary are one module, deliberately.**
:func:`recipes` is what a provider is handed so it can match the rest of the frame;
:func:`contain_row` is what `registry.Registry.draw` runs over what the provider hands
back. Written apart, the two were each asserted and disagreed from the day the recipes
landed (#604) against an escaping that had shipped two days before them (#550): charter
served ``ctx.chrome['heading']`` and then escaped it into the literal text
``\\x1b[1mMetrics\\x1b[0m`` on the way to the pane (#707). :func:`served_params` is the one
list both read, so a role added to :func:`_role_values` is served and admitted on the same
commit or on neither.

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

from .. import contain, tui
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


def dim_ok() -> bool:
    """Whether the frame may reduce the contrast of its own text — ``[frame] dim``.

    **One question asked in one place**, which is :func:`no_colour`'s discipline said about
    a different setting and for the same measured reason: :func:`recipes` answers it for a
    provider that composes with ``ctx.chrome["muted"]`` and `panel._write` answers it for
    every row charter's own renderers finished, and two readings of one key is how those
    two come to disagree about a frame nobody can see twice.

    **Why the key exists at all is a correction to this module**, and it is written down
    here rather than quietly fixed. :func:`_role_values` argues that its three attributes
    need no gate because "bold, dim and reverse are statements relative to whatever the
    operator's terminal already is". Bold and reverse survive that argument — bold adds
    weight and reverse is defined as the operator's own two colours exchanged, so neither
    can be wrong on a theme charter cannot see. Dim does not: it is relative in ONE
    direction, toward the background, and the direction is the whole problem. Charter's
    eight recipes were chosen against a dark terminal, where a dim grey has the whole
    distance to the background to spend. A plane that paints its panes light spends that
    distance the other way, and the operator reported the muted rows unreadable on
    ``bg = "brightblack"`` — a word their theme renders as a light tan.

    Read through `config.FRAME` at call time rather than cached, exactly as
    `slots.verbosity` and `slots.pad_of` read the keys they need: a panel is a long-lived
    process and `charter.toml` is re-read when the frame relaunches. The import is
    function-level for `_role_values`' own reason — this module is on a panel's repaint
    path and `config` resolves a plane at import.
    """
    from .. import config, instance
    return instance.look_of(config.FRAME).dim


#: The SGR parameters that INTRODUCE an extended colour, and therefore the ones whose
#: following parameters are not attributes at all.
#:
#: 38 (foreground), 48 (background) and 58 (underline colour) are each followed by a colour
#: SPACE and then that space's arguments: ``5;<n>`` for one of the 256, ``2;<r>;<g>;<b>``
#: for a 24-bit triple. The truecolor spelling is the hazard :func:`undim` exists to not
#: create — its space parameter is the digit **2**, and a pass that deleted every 2 from a
#: parameter list would turn ``38;2;255;0;0`` into ``38;255;0;0`` and hand a terminal a
#: foreground it never asked for.
#:
#: **Charter writes none of these** — `instance.FRAME_PANE_COLOURS` argues at length why
#: charter names the sixteen and never indexes the cube — and that is exactly why the case
#: is real rather than theoretical: :func:`undim` runs on a row a PROVIDER's component
#: finished, which is ordinary Python that may write whatever its author's terminal
#: supports, and `tui.sanitize` passes any ``ESC[<digits and semicolons>m`` through
#: untouched.
_EXTENDED_COLOUR = frozenset({38, 48, 58})

#: The SGR parameter that turns dim on. Named for `resets_everything`'s reason: the
#: property is the NUMBER, and ``ESC[02m`` and ``ESC[1;2m`` are both dim.
_DIM_PARAM = 2


def _without_dim(params: str) -> str | None:
    """The SGR parameter list *params* with every dim removed — ``None`` when the whole
    escape should go.

    **The numeric reading again** (:func:`resets_everything`), because this is the same
    mistake with a different number: "an SGR that turns dim on" is not the string
    ``\\x1b[2m``. It is a parameter list containing a parameter whose numeric value is two,
    and leading zeros are legal digits, and a parameter may sit anywhere in the list.
    Measured against the spellings that reach a pane::

        params        string-match ("2")   numeric, position-aware
        '2'           True                 dropped
        '02'          False  <- MISSED     dropped
        '1;2'         False  <- MISSED     '1'
        '2;32'        False  <- MISSED     '32'
        '38;2;255;0;0' False               '38;2;255;0;0'  <- the colour SPACE, kept

    The last row is why this walks the list rather than filtering it
    (:data:`_EXTENDED_COLOUR`).

    ``None`` rather than ``""`` for a list that emptied, and the difference is the whole
    correctness of the pass: ``\\x1b[m`` is an SGR with an omitted parameter, and an
    omitted parameter takes SGR's default of **zero** — so emitting it for a ``\\x1b[2m``
    that had nothing else in it would replace "turn dim on" with "turn EVERYTHING off",
    cancelling the colour of whatever span the row was in the middle of. A list that still
    holds a parameter comes back as a string even when that string is empty, because
    ``\\x1b[2;m`` really did carry a reset and the reset survives.
    """
    kept: list[str] = []
    pieces = params.split(";")
    i = 0
    while i < len(pieces):
        # Every piece is digits or empty — `_SGR` admits nothing else — so `int` cannot
        # raise, and an empty piece is spelled as the zero it means.
        value = int(pieces[i] or "0")
        if value in _EXTENDED_COLOUR:
            space = int(pieces[i + 1] or "0") if i + 1 < len(pieces) else -1
            run = {5: 3, 2: 5}.get(space, 1)
            kept.extend(pieces[i:i + run])
            i += run
            continue
        if value != _DIM_PARAM:
            kept.append(pieces[i])
        i += 1
    return ";".join(kept) if kept else None


def undim(row: str) -> str:
    """*row* with every dim taken out of it and nothing else touched.

    **A DELETION and never a substitution**, which is what makes it honest to run over a
    row charter did not write. `plain` already deletes every SGR from a provider's finished
    row when the operator asked for no colour, and this is the same class of answer about a
    smaller question: it removes an instruction to reduce contrast and leaves every colour
    the component chose exactly where it put it. A pass that remapped a provider's green to
    some other colour would be charter deciding what somebody else's component means, which
    is not a thing `[frame] dim` was asked for.

    **On the FINISHED row, in `panel._write`, which is the one place anything reaches a
    pane's screen.** That siting is the whole of why one key reaches the whole frame:
    charter's own renderers spell `statusline._DIM` at some forty call sites in
    `frame/slots.py` and a provider's component spells whatever it likes, and neither has
    to be told. It is the same argument `_write`'s own docstring already makes about
    `NO_COLOR`.

    Takes a finished row and hands back a string that must not go back through `tui` — the
    module docstring's ordering rule, unchanged. It changes no cell's width: every SGR
    costs zero columns, and this only ever makes one shorter or removes it.

    **There is no `if "\\x1b" not in row` shortcut in front of this, and the deletion is a
    measurement rather than a preference.** One was written first, copying
    `tui.strip_ansi`'s own, and the mutation sweep found it surviving. Timed on this
    machine over 200 000 calls each, the two rows a repaint actually carries::

        row with no escape   guard 0.032us   no guard 0.111us   saves 0.079us
        painted row          guard 1.933us   no guard 1.977us   saves 0.044us

    3.5x on the first row reads like a reason until it is spent: at 0.079us a line, a
    50-line repaint saves **4us against `render`'s own measured 4816us** — under a tenth of
    a percent, on a path that only runs at all for a plane that turned `dim` off. And most
    rows in this frame are not that row: the sidebar and the repo table colour nearly every
    line they compose. `re.sub` on a string it does not match returns that string, so the
    guard could not change an outcome either — which makes it the shape #568's sweep
    deletes, and "equivalent mutant" and "dead code" one finding rather than two.
    """
    return _SGR.sub(
        lambda m: "" if (kept := _without_dim(m.group(1))) is None else f"\x1b[{kept}m",
        row)


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
    `[frame] chrome` gate: bold and reverse are statements relative to whatever the
    operator's terminal already is, and ``green``/``yellow``/``red`` are slots in their
    own palette rather than colours charter picked out of the 256.

    **That argument used to name three attributes and it was wrong about one of them.**
    Dim is relative in exactly one direction — toward the background — and the direction is
    the defect: these values were chosen against a dark terminal, where a dim grey has the
    whole distance to the background to spend, and a plane that paints its panes light
    spends it the other way. The operator reported ``muted`` unreadable on
    ``bg = "brightblack"``, a word their own theme renders as a light tan. So dim has a
    gate now and the other two still do not — :func:`dim_ok`, honoured here and in
    `panel._write`, which is where the rest of that correction is written down.

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

    **``[frame] dim = false`` takes the dim out of every role that carries one**
    (:func:`dim_ok`), which today is ``muted`` alone and is asked as the property rather
    than by that name: :func:`undim` is run over the values, so a role added to
    :func:`_role_values` with a dim in it is covered on the day it is added instead of
    being the one that still reduces contrast on a plane that asked charter not to. It is
    also the same transform `panel._write` applies to the finished row, so the two cannot
    come to disagree about what "no dim" means.

    The interaction with the paragraph above is worth stating because it is an ordering:
    ``NO_COLOR`` empties every SGR role including this one, so a plane that turned dim off
    and an operator who turned colour off do not fight — the second is a superset of the
    first and wins by arriving at the same answer, not by being asked first.

    Emptied here rather than only stripped downstream by `panel._write`, even though the
    strip alone would produce the same pane. A provider handed a live ``\\x1b[2m`` for a
    role the frame is about to delete would be composing against a promise charter is not
    keeping — the same "convincing empty" this module refuses everywhere else — and a
    component that reads the role to decide something other than what to emit would decide
    it wrongly. It is also the cheaper half: nothing composed is nothing to strip.
    """
    from . import slots
    live = colour_ok()
    dim = dim_ok()
    out = {role: (sgr if dim else undim(sgr)) if live else ""
           for role, sgr in _role_values().items()}
    out["inset"] = slots._inset()
    return MappingProxyType(out)


def served_params() -> frozenset[int]:
    """Every SGR parameter charter's own recipes are made of, read as NUMBERS.

    **Derived from :func:`_role_values`, never spelled beside it.** A second list of the
    numbers is exactly how a role comes to be *served and not admitted* — a provider
    handed a colour that is escaped on the way out, which is #707 — or *admitted and not
    served*, which is a vocabulary charter enforces and nobody documents. One source, and
    a role added above widens this on the same commit.
    """
    out: set[int] = set()
    for value in _role_values().values():
        for m in _SGR.finditer(value):
            out.update(int(p or "0") for p in m.group(1).split(";"))
    return frozenset(out)


def is_recipe(params: str, served: frozenset[int]) -> bool:
    """True when every parameter in the SGR list *params* is one of *served*.

    **The vocabulary is a parameter and not a call, because this is on the repaint
    path.** Every escape in every row of every foreign pane asks this, and
    :func:`served_params` builds a set out of a function-level import — measured at 4.2µs,
    which is 1.2ms per repaint of one 24-row pane carrying a dozen escapes a row, against
    `render`'s own measured 4 816µs. So :func:`contain_row` asks once and hands the answer
    down. Required rather than defaulted: a second way of getting the vocabulary is how
    one caller comes to be asking about a different one.

    **This is not "did charter hand this exact string over", and that distinction is the
    whole design of :func:`contain_row`.** A string carries no provenance: the row a
    provider returns is text, and ``ctx.chrome["heading"]`` and a hard-coded
    ``"\\x1b[1m"`` are the same six characters by the time charter sees them. Asking
    which one it was is unanswerable, and answering it by matching the recipe's spelling
    would be this repo's own recurring defect (#547, #558, #537, #498, #577, #594) in the
    file that is about that defect.

    So the question asked is what the escape DOES, and it is asked of the parameters as
    numbers the way :func:`resets_everything` asks its own. Measured, the spellings that
    a substring match against `_role_values()` gets wrong::

        spelling         params    equals a served recipe   every parameter served
        '\\x1b[1m'        '1'       True                     True
        '\\x1b[01m'       '01'      False   <- MISSED         True
        '\\x1b[1;32m'     '1;32'    False   <- MISSED         True
        '\\x1b[m'         ''        False   <- MISSED         True   (empty is 0)
        '\\x1b[38;5;236m' '38;5;2…' False                     False
        '\\x1b[41m'       '41'      False                     False

    The two ``MISSED`` rows are a provider composing charter's own vocabulary in a legal
    spelling charter does not happen to write, and there is no argument for escaping
    those and keeping ``\\x1b[1m``: bold is bold. The two false rows are the property
    doing its job — a cube index and a background colour are colours charter did not get
    from the operator's palette, which is `instance.FRAME_CHROME`'s rule reaching the one
    place a stranger's code would otherwise sit outside it.
    """
    return all(int(p or "0") in served for p in params.split(";"))


def _pieces(row: str, served: frozenset[int]):
    """*row* as ``(text, keep)`` pairs: what charter passes through, and what it escapes.

    A non-recipe SGR yields no pair of its own — it stays inside the surrounding text and
    is escaped with it, which is what makes ``\\x1b[38;5;236m`` come out as the six
    visible characters of its own escape rather than as a colour with a hole in it.

    **A row with no recipe in it yields exactly one pair, the whole row**, which is what
    makes :func:`contain_row` answer such a row with `contain.one_line`'s own string,
    character for character. The containment did not change for anything outside the
    vocabulary; it grew a hole exactly the size of it.
    """
    pos = 0
    for m in _SGR.finditer(row):
        if is_recipe(m.group(1), served):
            yield row[pos:m.start()], False
            yield m.group(0), True
            pos = m.end()
    yield row[pos:], False


def _escaped(piece: str, budget: int) -> str:
    """*piece* with nothing in it that can forge a row — `contain.one_line`, or its answer.

    `str.isprintable()` is a C-level scan, and it is false for a SUPERSET of what
    `contain.one_line` rewrites: every *Other* and *Separator* category, against
    `one_line`'s `_INVISIBLE` five (``Cc``, ``Cf``, ``Cs``, ``Zl``, ``Zp``) plus whitespace
    that is not an ASCII space. Superset is the direction that makes this safe — printable
    implies `one_line` would change nothing, so such a piece inside its budget IS its own
    containment, and a `Co` or a `Zs` merely falls through to the slow path and gets the
    same answer a character at a time. `tui.sanitize` takes the same shortcut against the
    same question, and this is on the same repaint path.
    `TheBudgetNeedsNoSecondGuard` puts one of each through both.
    """
    # **The deletion sweep reports this line, twice, and it is right to.** A shortcut
    # cannot change an OUTPUT — that is what makes it a shortcut — so `collapse-ifexp`
    # and `shift-boundary` on it are equivalent mutants by construction and no test can
    # ever pin them. What it changes is cost, and the numbers are why it stays. One
    # 24-row foreign pane, `registry._fit` end to end, with the shortcut and without::
    #
    #     24 rows of 300 plain characters   340 us   without: 750 us   (was 512 us)
    #     24 rows, 4 escapes each           323 us   without: 397 us   (was  73 us)
    #     24 rows, no escape at all         205 us   without: 268 us   (was  65 us)
    #
    # The first row is the one that decides it: without the shortcut a long plain row
    # costs MORE than the `contain.one_line` this whole function replaces, so #707 would
    # have bought a provider its colour by making every text-heavy pane slower than it
    # was. `tui.sanitize` and `tui.width` carry the same shortcut against the same kind
    # of scan and for the same reason.
    return (piece if len(piece) <= budget and piece.isprintable()
            else contain.one_line(piece, limit=budget))


def contain_row(row: str, *, limit: int) -> str:
    """*row* from a foreign component, contained: charter's own vocabulary and nothing else.

    **The containment stays and the channel opens, and those are not in tension.** What
    `registry.Registry.draw` has to prevent is a stranger's row moving the cursor out of
    its own rectangle, erasing a pane it does not own, or naming a colour the operator's
    palette never chose. None of those is an SGR from :func:`served_params` — an SGR
    costs zero columns and says nothing about position, and charter's seven roles are
    plain attributes and the sixteen palette names. Everything else in the row goes
    through `contain.one_line` exactly as the whole row used to: a cursor move, an OSC
    title string, a bare BEL and a newline all come out as their own visible escape.

    So this is the same guarantee with one hole cut in it, and the hole is the size of
    the vocabulary charter already publishes. Before it, `docs/frame.md`'s own worked
    example — a provider writing ``ctx.chrome['heading']`` — reached the pane as the
    literal text ``\\x1b[1mMetrics\\x1b[0m`` (#707): charter served a colour channel and
    then stripped it, and the two halves each had a test while the round trip had none.

    **Under `NO_COLOR`, or a pane that is not a terminal, every escape is escaped again.**
    :func:`recipes` already serves the empty string there, so a component built on the
    recipes emits no SGR at all and this line changes nothing for it. What it catches is
    the component that hard-coded ``\\x1b[1m``: §3.2's rule is that charter emits no SGR
    from the frame, and passing one through on a provider's behalf is charter asking
    somebody else to paint — the half of that promise `no_colour`'s own docstring is
    about.

    *limit* is `registry.LINE_LIMIT`, and it bounds the CHARACTERS, which
    `tui.truncate`'s visible-width clamp cannot: a line of a million combining marks
    measures zero cells. **A kept escape spends that budget like anything else**, and the
    one break below is what that costs: an SGR's parameter list is unbounded, so
    ``\\x1b[`` + ``1;`` four hundred times + ``m`` is every parameter in the vocabulary
    and is no colour any operator will ever see. It is taken whole or not at all, because
    half of ``\\x1b[1m`` is a bare ESC and manufacturing one is the single thing an
    escape-aware containment must never do.

    **What it costs, measured, because this is the repaint path.** One 24-row foreign
    pane, `registry._fit` end to end, against the `contain.one_line`-over-the-whole-row it
    replaces::

        24 rows, 4 escapes each (the docs' own example)   323 us   was   73 us
        24 rows, 12 escapes each (deliberately dense)     914 us   was  457 us
        24 rows, no escape at all                         205 us   was   65 us
        24 rows of 300 plain characters                   340 us   was  510 us

    The last row is not a typo: :func:`_escaped`'s `isprintable()` scan is faster than
    `one_line`'s per-character loop, so a long plain row now costs less than it did. The
    worst case is half a millisecond against `render`'s own measured 4 816 us — a tenth of
    what the pane already spends to have rows at all — and it is paid only by a pane
    charter did not write. The hoist that made it that rather than four times that is
    :func:`is_recipe`'s: the vocabulary is built once here and not once per escape.

    **An ``if used >= limit: break`` above that line was written first and then deleted**,
    and the reason is the sweep's rather than tidiness: it was there to stop
    `contain.one_line` being handed a negative limit — which slices from the END of the
    string rather than refusing — and it could not, because :func:`_pieces` strictly
    alternates and a kept piece is appended only when it FITS. So an escaped piece is
    always preceded either by nothing or by a kept piece that left ``used <= limit``, and
    the subtraction is never negative. `TheBudgetNeedsNoSecondGuard` pins that property
    without the guard, over every row a fuzz could build.
    """
    if not colour_ok():
        return contain.one_line(row, limit=limit)
    served = served_params()
    out: list[str] = []
    used = 0
    for piece, keep in _pieces(row, served):
        text = piece if keep else _escaped(piece, limit - used)
        if keep and used + len(text) > limit:
            break
        out.append(text)
        used += len(text)
    return "".join(out)


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
