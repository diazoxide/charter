"""Tiny stdlib-only terminal-layout kit — "rich-lite" for charter's TUI output.

Why not rich/textual? The status line re-renders on *every* prompt, so it must
import in milliseconds and add no runtime dependency (`charter` ships stdlib-only).
This module is the small layout algebra that covers what charter draws —
ANSI-aware width math plus composable Text/Row/Stack/Columns nodes — so column
arithmetic lives in one tested place instead of ad-hoc padding at call sites.

The one hard guarantee, enforced by every node: **no rendered line ever
exceeds the requested width** (visible columns; ANSI SGR escapes count as
zero). Overflow is truncated with an ellipsis, never wrapped — a single
wrapped line shears every column below it. Rendered lines also never carry
trailing whitespace, even when it hides behind trailing colour escapes.

That guarantee is about the terminal, so it cannot be computed from a string the
terminal reads differently: **SGR is the only escape this module passes through**, and
`sanitize` drops everything else — a cursor move, an erase, an OSC title string, a bare
control character — before anything is measured or clamped. Charter's markup is charter's
own; the values interpolated into it (a forge's JSON, a directory name) are not (#326).

Vocabulary::

    sanitize                  drop everything that is not charter's own markup
    width / truncate / pad    ANSI-aware string primitives
    column                    how wide one table column has to be for its values
    Text                      markup line(s), clamped at render time
    Cell + Row                one line of fixed/natural-width cells
    Stack                     vertical composition of nodes
    Columns                   side-by-side columns of independent heights

Every node renders via ``node.render(width) -> list[str]``. Contents are
"markup": plain text freely interleaved with ANSI SGR colour escapes.
"""

from __future__ import annotations

import os
import re
import unicodedata
from collections.abc import Iterable, Sequence

RESET = "\x1b[0m"
ELLIPSIS = "…"

#: The environment variables that describe **the terminal a process was launched from**,
#: as opposed to the one it is drawing into. :func:`term_width` reads the first of them;
#: `commands_frame._frame_env` strips both out of every child a frame starts, because a
#: captured size is wrong the moment the child is a different rectangle.
#:
#: A named constant rather than two string literals at each of those places, and the
#: reason is `charter.legacyenv`'s verbatim: **a tuple of strings was never what needed a
#: plane.** `tests/_envguard.py` removes the shell's own answers from the suite before
#: `charter.config` is first imported — before a plane is resolved out of the operator's
#: shell — so it cannot ask `commands_frame`, which pulls `config` in at import. It can
#: ask this module, which imports ``os``, ``re``, ``unicodedata`` and nothing else. Asked
#: rather than re-spelled, a third geometry variable is covered by that scrub on the
#: commit that adds it instead of on the commit that debugs it (#544).
#:
#: ``LINES`` is here although nothing in charter reads it yet, for the reason
#: `tests._envguard._scrubbed_names` gives about `session._WINDOW_ID_VARS`: "deliberately
#: not read" is a decision that can change, and a scrub costs nothing if it is wrong.
#: `commands_frame` already strips it, so the pair is charter's own answer already.
TERMINAL_SIZE_VARS = ("COLUMNS", "LINES")

_SGR = re.compile(r"\x1b\[[0-9;]*m")
#: Trailing whitespace hiding *behind* trailing SGR escapes ("a \x1b[0m").
_HIDDEN_TRAIL = re.compile(r"[ \t]+((?:\x1b\[[0-9;]*m)+)$")

#: Everything this module may be handed, split into "charter's own markup" and "not".
#: Alternation order is the whole design: SGR is matched FIRST and handed back
#: untouched, so the catch-all `\x1b` at the end can delete a stray introducer without
#: decapitating a colour span and spilling `[32m` onto the screen as text.
#:
#: The rest, in the order a terminal would parse it. The *string* sequences (OSC, DCS,
#: APC, PM, SOS) come before the single-character forms because they must go whole: drop
#: only the introducer and the payload — `0;pwned` — prints as ordinary text, which is
#: worse than the escape because it looks like data. An unterminated one is removed to
#: the end of the string for that same reason.
#:
#: `\x1b[^\x1b]` catches every remaining two-character escape, `ESC c` (RIS, a full
#: terminal reset) among them, and stops short of a following ESC so that `ESC ESC [32m`
#: loses the stray introducer and keeps the colour — which is what a terminal does with
#: it too.
_MARKUP_OR_CONTROL = re.compile(
    r"(\x1b\[[0-9;]*m)"                            # 1 — SGR: charter's own, kept
    r"|\x1b[\]P^_X][^\x1b\x07]*(?:\x07|\x1b\\|$)"  # OSC/DCS/APC/PM/SOS … ST | BEL | end
    r"|\x1b\[[0-?]*[ -/]*[@-~]"                    # any other CSI (cursor, erase, …)
    r"|\x1b[^\x1b]"                                # other two-character escapes
    r"|\x1b"                                       # a lone/trailing ESC
    r"|([\t\n\r\v\f])"                             # 2 — whitespace controls → a space
    r"|[\x00-\x1f\x7f-\x9f]"                       # other C0, DEL, C1: removed
)


def _keep(m: "re.Match[str]") -> str:
    if m.group(1):
        return m.group(1)
    # A tab jumps to the next tab stop and a newline ends the line — both shear the
    # columns below, which is the one thing this module promises not to do. They keep
    # their separation as a single space rather than vanishing, so `a\tb` stays two
    # words instead of becoming one.
    return " " if m.group(2) else ""


# --------------------------------------------------------------------------
# String primitives
# --------------------------------------------------------------------------

def sanitize(s: str) -> str:
    """Return *s* with everything that is not charter's own colour markup removed.

    `tui` measures and clamps *markup*, and its contract has always been that markup
    is charter's: SGR escapes, which cost zero columns, and text, which costs one
    column per character. Nothing enforced that. A forge's JSON, a directory name off
    the filesystem — anything that reaches a `Cell` — could carry an erase-in-display,
    an OSC title string or a bare BEL, and the module would count it as visible
    columns, pad around a width the terminal disagreed with, and copy it to a surface
    that repaints every ten seconds with no human in the loop (#326).

    Deliberately kept as a *narrow* removal rather than a whitelist of printable text.
    This module exists to emit SGR — a blanket strip would fight the colour it is built
    to produce — so SGR is matched first and passed through untouched, and only what a
    terminal would interpret as a command rather than as content is dropped. Sanitising
    here rather than at each call site is what makes the guarantee hold for a value
    nobody has thought about yet, including the next field somebody adds.

    Not a substitute for validating at the boundary: `glstate` still refuses a change
    identifier that is not a number, because a value with the wrong *type* is a defect
    worth catching where it enters, not a rendering detail. This is the second half.
    """
    # `isprintable()` is a C-level scan and false only for a control character (ASCII
    # space excepted), so the overwhelmingly common case never touches the regex.
    return s if s.isprintable() else _MARKUP_OR_CONTROL.sub(_keep, s)


def strip_ansi(s: str) -> str:
    """Return *s* as the plain text a terminal would show: SGR escapes removed, and —
    since anything else that claims to be an escape is not charter's markup either —
    :func:`sanitize` applied first."""
    s = sanitize(s)
    return _SGR.sub("", s) if "\x1b" in s else s


def _char_width(ch: str) -> int:
    """Terminal cell width of one character (0 combining, 2 east-asian wide)."""
    if ch < "\x80":
        return 1
    if unicodedata.combining(ch):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def width(s: str) -> int:
    """Visible terminal width of markup *s* (ANSI stripped, wide chars = 2)."""
    s = strip_ansi(s)
    if s.isascii():  # fast path: the overwhelmingly common case
        return len(s)
    return sum(map(_char_width, s))


def truncate(s: str, w: int, ellipsis: str = ELLIPSIS) -> str:
    """Clamp markup *s* to at most *w* visible columns.

    Returns *s* unchanged when it already fits (fast path). When cut, ANSI
    escapes are carried through verbatim (so colour spans stay intact),
    *ellipsis* marks the cut, and a reset is appended so open styles never
    bleed into whatever is printed next.
    """
    if w <= 0:
        return ""
    # Before the fits-already fast path, not after: returning *s* unchanged is exactly
    # how an escape reached the screen without anything ever having looked at it (#326).
    s = sanitize(s)
    if width(s) <= w:
        return s
    keep = max(0, w - width(ellipsis))
    out: list[str] = []
    vis = i = 0
    n = len(s)
    while i < n:
        if s[i] == "\x1b":
            m = _SGR.match(s, i)
            if m:  # copy the whole escape verbatim (zero visible width)
                out.append(m.group())
                i = m.end()
                continue
        cw = _char_width(s[i]) if s[i] >= "\x80" else 1
        if vis + cw > keep:
            break
        out.append(s[i])
        vis += cw
        i += 1
    out.append(ellipsis)
    if "\x1b" in s:
        out.append(RESET)
    return "".join(out)


def pad(s: str, w: int, align: str = "left") -> str:
    """Fit markup *s* to exactly *w* visible columns (truncate long, pad short).

    Padding is plain spaces appended outside any colour span, so styles such
    as underline never leak into the padding. *align* is ``left`` / ``right``
    / ``center``.
    """
    s = truncate(s, w)
    fill = w - width(s)
    if fill <= 0:
        return s
    if align == "right":
        return " " * fill + s
    if align == "center":
        left = fill // 2
        return " " * left + s + " " * (fill - left)
    return s + " " * fill


def column(header: str, cells: Iterable[str], gap: int = 2,
           cap: int | None = None) -> int:
    """Visible width for one table column: its *header*, its widest cell, and *gap*.

    The one number a hand-rolled table gets wrong. A column written as ``{name:<28}``
    is two mistakes at once, and this function is the answer to both:

    * **28 is a guess about content.** ``:<28`` pads a short value and does nothing at
      all to a long one — it pushes, so that row's remaining columns land somewhere no
      other row's do and the table stops being a table. The width has to come from the
      values about to be printed, which is what *cells* is. Sizing every column this
      way — the numeric ones too — means the report cannot be pushed out of alignment
      by any value at all, rather than by none of the values somebody thought of.
    * **``:<28`` counts characters.** A terminal lays out *cells*: a CJK name is two
      per glyph and a combining mark is zero, so `len` over-pads the first and
      under-pads the second, and a name well inside the budget still shifts its row.
      Every measurement here is :func:`width`, and callers pad with :func:`pad` for the
      same reason — the two have to agree or the arithmetic was for nothing.

    *gap* is the separation from the next column, counted inside the returned width so
    a caller pads once rather than padding and then adding spaces. Passing a *cap*
    bounds the content (never the gap) for a column whose values are prose rather than
    identifiers; the cells then truncate with an ellipsis inside their column instead of
    widening it. Leave it ``None`` where the reader has to be able to read the value
    back off the row — a clipped name is one they cannot go and act on.

    What this leaves unanswered is whether a cell is **legible**. :func:`width` reports the
    cells a value *declares*, which is what alignment is made of; three U+3164 HANGUL
    FILLERs declare six and render as nothing (#498). Use `contain.readable` where the
    question is whether a reader can name the value, not where it sits.
    """
    w = max([width(header)] + [width(c) for c in cells])
    return (w if cap is None else min(w, cap)) + gap


def _env_columns() -> int | None:
    """``$COLUMNS`` as a number, or ``None`` when it is unset or is not one.

    Answers what the variable *says*, never whether it is usable — see
    :func:`term_width` for why that judgement is made in exactly one place.
    """
    try:
        return int(os.environ[TERMINAL_SIZE_VARS[0]])
    except (KeyError, ValueError):
        return None


def _tty_columns() -> int | None:
    """The tty's own column count, or ``None`` when there is no tty to ask.

    Same contract as :func:`_env_columns`: a number or nothing, and no opinion about
    whether the number is a width.
    """
    try:
        return os.get_terminal_size().columns
    except OSError:
        return None


def term_width(default: int = 80, floor: int = 1) -> int:
    """Terminal width: ``$COLUMNS`` (:data:`TERMINAL_SIZE_VARS`), else the tty size,
    else *default*.

    Clamped to at least *floor*. Status-line style programs get their size via
    ``$COLUMNS`` because stdout is a pipe, hence the env-first order.

    **A width is a positive number, whichever source produced it — and that is asked
    once, here, rather than by each source about itself.** The env branch used to carry
    its own ``if w <= 0: raise ValueError(w)``, because a real environment in this
    project exports ``COLUMNS=0`` and ``int("0")`` parses happily: ``max(floor, 0)``
    turned a meaningless value into a plausible-looking floor-width render, and the whole
    status line squeezed into 24 columns. The guard was right and it was attached to a
    **spelling** — ``$COLUMNS`` being zero — instead of to the property, so one rung
    lower on the same ladder the identical value walked straight through: a tty that
    reports zero columns handed back ``max(1, 0) == 1`` and charter drew every table,
    row and panel one column wide (#594).

    That tty is ordinary, not exotic. A pty created without a window size reports zero
    until someone calls ``TIOCSWINSZ`` — which is what :func:`os.openpty` gives you, and
    so what tooling, some CI shells and a terminal attached before its size is negotiated
    all give you. The suite has been carrying the evidence: under a real pty, 28 failures
    across 8 modules were all terminal-size, and giving that pty a size made all 28 pass.

    So each source answers with a number or with nothing (:func:`_env_columns`,
    :func:`_tty_columns`), the *answer* is what is judged, and a source with no usable
    answer is indistinguishable from a source with no answer at all. Adding a third
    source is then one entry in the tuple rather than a fourth place to remember the
    zero. *default* is not asked the question: it is the caller's own stated fallback for
    "nothing could be measured", and *floor* is the last word on all three paths.
    """
    for ask in (_env_columns, _tty_columns):
        w = ask()
        if w is not None and w > 0:
            return max(floor, w)
    return max(floor, default)


# Internal aliases: node methods take a ``width`` parameter by API contract,
# which shadows the module-level function inside them.
_width = width
_truncate = truncate
_pad = pad


def _finish(line: str) -> str:
    """Strip trailing whitespace, including any hiding behind trailing escapes."""
    line = line.rstrip(" \t")
    while True:
        cut = _HIDDEN_TRAIL.sub(r"\1", line)
        if cut == line:
            return line
        line = cut.rstrip(" \t")


# --------------------------------------------------------------------------
# Layout nodes
# --------------------------------------------------------------------------

class Node:
    """Base layout node.

    A node renders to a list of finished lines: each is guaranteed to be at
    most *width* visible columns (truncated with ``…``, never wrapped) and to
    carry no trailing whitespace.
    """

    __slots__ = ()

    def render(self, width: int) -> list[str]:
        """Render to lines, each clamped to *width* visible columns."""
        raise NotImplementedError


class Text(Node):
    """One or more lines of raw markup, clamped at render time.

    Embedded newlines split into multiple lines; each is clamped separately.
    """

    __slots__ = ("markup",)

    def __init__(self, markup: str = "") -> None:
        self.markup = markup

    def render(self, width: int) -> list[str]:
        return [_finish(_truncate(ln, width)) for ln in self.markup.split("\n")]


class Cell:
    """A :class:`Row` ingredient: markup with an optional fixed visible width.

    ``width=None`` keeps the natural width (no padding or truncation — the
    row still clamps the assembled line). A fixed ``width`` pads/truncates
    the cell to exactly that many columns, aligned by *align*.
    """

    __slots__ = ("markup", "width", "align")

    def __init__(self, markup: str, width: int | None = None,
                 align: str = "left") -> None:
        self.markup = markup
        self.width = width
        self.align = align


class Row(Node):
    """A single line of cells joined by *gap*, clamped to the render width.

    Cells may be :class:`Cell` instances or plain markup strings (treated as
    natural-width cells). Fixed-width cells keep a table's columns aligned
    across sibling rows without any call-site padding math.
    """

    __slots__ = ("cells", "gap")

    def __init__(self, *cells: Cell | str, gap: str = " ") -> None:
        self.cells = tuple(c if isinstance(c, Cell) else Cell(c) for c in cells)
        self.gap = gap

    def render(self, width: int) -> list[str]:
        parts = [
            c.markup if c.width is None else _pad(c.markup, c.width, c.align)
            for c in self.cells
        ]
        return [_finish(_truncate(self.gap.join(parts), width))]


class Stack(Node):
    """Vertical composition: children's lines concatenated top to bottom.

    Children may be nodes or plain markup strings (coerced to :class:`Text`).
    """

    __slots__ = ("children",)

    def __init__(self, *children: Node | str) -> None:
        self.children = tuple(
            c if isinstance(c, Node) else Text(c) for c in children
        )

    def render(self, width: int) -> list[str]:
        lines: list[str] = []
        for child in self.children:
            lines.extend(child.render(width))
        return lines


class Columns(Node):
    """Side-by-side columns of **independent heights**.

    ``columns`` is a sequence of ``(content, width)`` pairs — *content* a
    :class:`Node` or a plain list of markup lines, *width* a fixed visible
    column width or ``None`` for a flex column that shares whatever space
    remains after fixed columns and gaps. Shorter columns are blank-filled so
    rows stay aligned however tall each column is; every assembled row is
    clamped to the render width, so an over-budget spec degrades to truncated
    lines instead of wrapping.
    """

    __slots__ = ("columns", "gap")

    def __init__(self, columns: Sequence[tuple[Node | Sequence[str], int | None]],
                 gap: str = "  ") -> None:
        self.columns = list(columns)
        self.gap = gap

    def render(self, width: int) -> list[str]:
        if not self.columns:
            return []
        gap_w = _width(self.gap)
        fixed = sum(w for _, w in self.columns if w is not None)
        flexible = sum(1 for _, w in self.columns if w is None)
        spare = max(0, width - fixed - gap_w * (len(self.columns) - 1))
        share, extra = divmod(spare, flexible) if flexible else (0, 0)

        widths: list[int] = []
        for _, w in self.columns:
            if w is None:
                w = share + (1 if extra > 0 else 0)
                extra -= 1
            widths.append(w)

        blocks = [
            content.render(w) if isinstance(content, Node)
            else [_finish(_truncate(ln, w)) for ln in content]
            for (content, _), w in zip(self.columns, widths)
        ]
        height = max(map(len, blocks))
        out: list[str] = []
        for i in range(height):
            row = self.gap.join(
                _pad(block[i] if i < len(block) else "", w)
                for block, w in zip(blocks, widths)
            )
            out.append(_finish(_truncate(row, width)))
        return out
